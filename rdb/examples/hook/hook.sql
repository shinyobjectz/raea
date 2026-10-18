-- The hook, built in one go: its rows, then its rules. snippets.sql builds the same edit one move at a time.
-- Kinds are tables; their columns are components. Every frame starts from these rows and the keys.
INSERT INTO edit VALUES (30, 1080, 1920, 150);

INSERT INTO camera (id, x, y, z, look_y, bg) VALUES ('cam', 0, 2.2, 9, 1, '#1b1f3a');
INSERT INTO light  (id, dir_x, dir_y, dir_z, lux) VALUES ('sun', -0.5, -1, -0.4, 9000);
INSERT INTO plane  (id, size, color) VALUES ('floor', 30, '#2a2440');
INSERT INTO cube   (id, size, color, y, rough) VALUES ('hero', 1.6, '#ff7a59', 1.4, 0.3);
INSERT INTO sphere (id, class, size, color, y, rough, metal)
  SELECT 'ring' || i, 'ring', 0.38, '#ffd8c2', 0.6, 0.2, 0.3 FROM range(14) t(i);

INSERT INTO title (id, text, font, weight, size, color, y, opacity) VALUES
  ('hook', 'Stop scrolling', 'Avenir Next', 800, 120, '#fff8ec', 260, 0),
  ('sub', 'this changes your mornings', 'Avenir Next', 600, 52, '#ffcc66', 410, 0);
INSERT INTO caption (id, text, font, weight, size, color, y, height) VALUES
  ('cap', 'one serum, one step', 'Avenir Next', 700, 64, '#ffffff', 1480, 80);

-- perception data: a face that appears at frame 60 and rises (in a real edit this comes from the perception table)
INSERT INTO face (id, y, visible) VALUES ('face', 1520, 0);
INSERT INTO keys VALUES
  ('face', 'visible', 0, 0, 'step'), ('face', 'visible', 60, 1, 'step'),
  ('face', 'y', 60, 1520, 'linear'), ('face', 'y', 120, 1300, 'linear');

-- time for the titles
INSERT INTO keys VALUES
  ('hook', 'y', 0, 340, 'out'), ('hook', 'y', 18, 260, 'out'),
  ('sub', 'y', 14, 470, 'out'), ('sub', 'y', 32, 410, 'out');

-- behaviour: every frame, in this order
UPDATE cube SET rot_y = frame * 2 * pi() / 180 WHERE id = 'hero';
UPDATE sphere SET
  x = 2.4 * cos(2 * pi() * (row_number() OVER () / count(*) OVER () + frame * 0.004)),
  z = 2.4 * sin(2 * pi() * (row_number() OVER () / count(*) OVER () + frame * 0.004)),
  y = 0.6 + 0.5 * sin(2 * pi() * row_number() OVER () * 3 / count(*) OVER () + frame * 0.08)
  WHERE class = 'ring';
UPDATE sphere SET scale = 0.55 + 0.6 * y / max(y) OVER () WHERE class = 'ring';
UPDATE title SET opacity = least(1, frame / 14) WHERE id = 'hook';
UPDATE title SET opacity = least(1, (frame - 14) / 18) WHERE id = 'sub' AND frame >= 14;
UPDATE caption SET y = least(y, (SELECT min(y) FROM face WHERE visible = 1) - height - 24)
  WHERE (SELECT count(*) FROM face WHERE visible = 1) > 0;
-- the dodge eases, starting early enough to be clear when the face appears
INSERT INTO transition (kind, col, frames, ease, align) VALUES ('caption', 'y', 10, 'out', 'before');
