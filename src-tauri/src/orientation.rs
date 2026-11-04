use rusqlite::Connection;

use crate::db::queries;
use crate::error::AppResult;
use crate::types::{ContainerInfo, ResolvedRotation, Rotation};

pub const GLOBAL_KEY: &str = "global_rotation";

/// Model override, then pack override, then global default. First one set wins.
pub fn resolve(
    model: Option<Rotation>,
    pack: Option<Rotation>,
    global: Option<Rotation>,
) -> ResolvedRotation {
    let (rotation, source) = if let Some(r) = model {
        (r, "model")
    } else if let Some(r) = pack {
        (r, "pack")
    } else if let Some(r) = global {
        (r, "global")
    } else {
        (
            Rotation {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            "none",
        )
    };
    ResolvedRotation {
        rotation: normalise(rotation),
        source: source.into(),
    }
}

/// Keep each angle in [0, 360).
pub fn normalise(r: Rotation) -> Rotation {
    let n = |v: f64| {
        let m = v.rem_euclid(360.0);
        if (m - 360.0).abs() < 1e-9 {
            0.0
        } else {
            m
        }
    };
    Rotation {
        x: n(r.x),
        y: n(r.y),
        z: n(r.z),
    }
}

/// The pack a container belongs to: the nearest archive, or folder marked as a pack.
pub fn find_pack(conn: &Connection, container_id: i64) -> AppResult<Option<ContainerInfo>> {
    let mut next = Some(container_id);
    let mut hops = 0;
    while let Some(id) = next {
        let c = queries::get_container(conn, id)?;
        if c.kind != "folder" || c.is_pack {
            return Ok(Some(c));
        }
        next = c.parent_id;
        hops += 1;
        if hops > 512 {
            break;
        }
    }
    Ok(None)
}

pub fn global(conn: &Connection) -> AppResult<Option<Rotation>> {
    Ok(queries::get_setting(conn, GLOBAL_KEY)?.and_then(|s| serde_json::from_str(&s).ok()))
}

pub fn resolve_for_model(
    conn: &Connection,
    model_rot: Option<Rotation>,
    container_id: i64,
) -> AppResult<(ResolvedRotation, Option<ContainerInfo>)> {
    let pack = find_pack(conn, container_id)?;
    let resolved = resolve(
        model_rot,
        pack.as_ref().and_then(|p| p.rotation),
        global(conn)?,
    );
    Ok((resolved, pack))
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn r(x: f64, y: f64, z: f64) -> Rotation {
        Rotation { x, y, z }
    }

    #[test]
    fn precedence() {
        let m = Some(r(90.0, 0.0, 0.0));
        let p = Some(r(0.0, 90.0, 0.0));
        let g = Some(r(0.0, 0.0, 90.0));
        assert_eq!(resolve(m, p, g).source, "model");
        assert_eq!(resolve(None, p, g).source, "pack");
        assert_eq!(resolve(None, p, g).rotation, r(0.0, 90.0, 0.0));
        assert_eq!(resolve(None, None, g).source, "global");
        let none = resolve(None, None, None);
        assert_eq!(none.source, "none");
        assert_eq!(none.rotation, r(0.0, 0.0, 0.0));
    }

    #[test]
    fn a_zero_override_still_wins() {
        // An explicit 0,0,0 on the model means "this one is already right", not "inherit".
        let res = resolve(Some(r(0.0, 0.0, 0.0)), Some(r(90.0, 0.0, 0.0)), None);
        assert_eq!(res.source, "model");
        assert_eq!(res.rotation, r(0.0, 0.0, 0.0));
    }

    #[test]
    fn normalises_angles() {
        assert_eq!(normalise(r(360.0, -90.0, 450.0)), r(0.0, 270.0, 90.0));
    }

    #[test]
    fn pack_lookup_walks_up_to_archive_or_marked_folder() {
        let conn = crate::db::test_conn();
        conn.execute_batch(
            "INSERT INTO libraries (id, name, path) VALUES (1, 'L', '/l');
             INSERT INTO containers (id, library_id, parent_id, kind, path, name) VALUES
               (1, 1, NULL, 'folder', '/l', 'L'),
               (2, 1, 1, 'folder', '/l/marked', 'marked'),
               (3, 1, 2, 'folder', '/l/marked/inner', 'inner'),
               (4, 1, 1, 'zip', '/l/p.zip', 'p.zip');
             UPDATE containers SET is_pack = 1, rot_x = 90 WHERE id = 2;",
        )
        .expect("seed");
        assert_eq!(find_pack(&conn, 3).expect("pack").map(|c| c.id), Some(2));
        assert_eq!(find_pack(&conn, 4).expect("pack").map(|c| c.id), Some(4));
        assert_eq!(find_pack(&conn, 1).expect("pack").map(|c| c.id), None);
        let (res, _) = resolve_for_model(&conn, None, 3).expect("resolve");
        assert_eq!(res.source, "pack");
        assert_eq!(res.rotation.x, 90.0);
    }
}
