-- Archive entries now carry their own modified time. Re-list archives once to fill it in.
UPDATE containers SET signature = NULL WHERE kind <> 'folder';
