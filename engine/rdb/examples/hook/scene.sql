-- The hook's content: what is in the edit, as rows. Behaviour lives in hook.feature, lowered through steps.sql.
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
