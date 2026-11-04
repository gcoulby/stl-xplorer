-- Images and PDFs are indexed alongside meshes. model_count stays meshes only (it is what the
-- UI calls "models"); file_count counts everything indexed and decides whether a folder shows.
ALTER TABLE containers ADD COLUMN file_count INTEGER NOT NULL DEFAULT 0;
UPDATE containers SET file_count = model_count;

-- Auto tags: a tag with keywords is applied to every model whose path contains one of them.
-- model_tags.auto marks rows the rules own, so editing a rule never touches manual tags.
ALTER TABLE tags ADD COLUMN keywords TEXT;
ALTER TABLE model_tags ADD COLUMN auto INTEGER NOT NULL DEFAULT 0;

-- Re-list every archive on the next scan so images and PDFs inside them are picked up.
UPDATE containers SET signature = NULL WHERE kind <> 'folder';
