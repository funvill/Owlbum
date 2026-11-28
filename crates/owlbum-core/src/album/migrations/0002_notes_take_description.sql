-- Version 2 (owner's feedback, issue 208): the typed Description and Notes
-- are one field, Notes. Each photo's typed description is folded into its
-- notes once, so nothing typed is lost: the description, a blank line, then
-- the notes (or the description alone when there were no notes). The
-- `description` column stays, empty; the model's description (tags) is
-- apart and untouched.

-- The contentless search table cannot change one column, so the folded
-- photos' entries are dropped and written again from the rows below.
CREATE TEMP TABLE folded (id INTEGER PRIMARY KEY);
INSERT INTO folded (id)
SELECT id FROM photos WHERE description IS NOT NULL AND trim(description) != '';

DELETE FROM search WHERE rowid IN (SELECT id FROM folded);

UPDATE photos
   SET notes = CASE
                 WHEN notes IS NULL OR trim(notes) = '' THEN description
                 ELSE description || char(10) || char(10) || notes
               END,
       description = NULL
 WHERE id IN (SELECT id FROM folded);

-- An empty description left over holds nothing.
UPDATE photos SET description = NULL WHERE description IS NOT NULL;

INSERT INTO search (rowid, file_name, description, notes, people, place,
                    tag_description, keywords, objects)
SELECT p.id, p.file_name, p.description, p.notes,
       (SELECT group_concat(n.name, ' ') FROM photo_people pp
          JOIN people n ON n.id = pp.person_id WHERE pp.photo_id = p.id),
       p.loc_name,
       (SELECT t.description FROM tags t
          WHERE t.photo_id = p.id AND t.part = 'describe' AND t.status = 'done'
          ORDER BY t.made_at DESC LIMIT 1),
       (SELECT group_concat(l.label, ' ') FROM photo_labels l
          WHERE l.photo_id = p.id AND l.kind = 'keyword'),
       (SELECT group_concat(l.label, ' ') FROM photo_labels l
          WHERE l.photo_id = p.id AND l.kind = 'object')
  FROM photos p
 WHERE p.id IN (SELECT id FROM folded);

DROP TABLE folded;
