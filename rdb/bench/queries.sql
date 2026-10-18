-- The agent's read queries (G5's query tool: one SELECT over the kinds, keys, rule, asset, move), one per line.
-- bench/ref/queries.json holds DuckDB's answers on the live hook edit, recorded before phase 4 replaced DuckDB.
-- Values compare exactly, except sums and averages to 1e-12 relative: DuckDB's float summation order is its own.
SELECT * FROM cube
SELECT id, class, y FROM sphere ORDER BY id
SELECT id, size FROM sphere WHERE class = 'ring' AND size > 0.3 ORDER BY id DESC LIMIT 3
SELECT count(*) FROM sphere
SELECT class, count(*) AS n, avg(size) AS mean, max(y) AS top FROM sphere GROUP BY class ORDER BY class
SELECT * FROM keys ORDER BY id, prop, frame
SELECT id, prop, min(frame) AS first, max(frame) AS last, count(*) AS n FROM keys GROUP BY id, prop ORDER BY id, prop
SELECT name, sql FROM rule ORDER BY seq
SELECT name FROM rule WHERE sql LIKE '%sphere%' ORDER BY name
SELECT n, text FROM move ORDER BY n DESC LIMIT 5
SELECT count(*) AS moves FROM move
SELECT fps, width, height, frames FROM edit
SELECT frames / fps AS seconds FROM edit
SELECT id, text, size, y FROM caption ORDER BY y
SELECT text, color FROM title
SELECT id FROM sphere WHERE id IN ('ring0', 'ring3', 'ring9') ORDER BY id
SELECT id, y FROM sphere WHERE y BETWEEN 0.5 AND 1.5 ORDER BY y, id
SELECT DISTINCT class FROM sphere ORDER BY class
SELECT id, size * 2 AS double_size, upper(color) AS c FROM sphere WHERE class <> 'ring' ORDER BY id
SELECT kind, col, frames, ease, align FROM transition ORDER BY seq
SELECT count(*) FROM asset
SELECT id, lux FROM light
SELECT id, bg, look_y FROM camera
SELECT id, visible FROM face WHERE visible IS NOT NULL
SELECT sum(size) AS total, min(size) AS smallest FROM sphere WHERE class = 'ring'
SELECT id FROM sphere ORDER BY seq LIMIT 2 OFFSET 1
