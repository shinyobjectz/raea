# run.py's geometry on synthetic data (its functions taken out of the script, which runs a solve when executed):
#   ~/.cache/raea-matchmove/.venv/bin/python -I tools/matchmove/test_run.py
import ast, math, os, sys

import numpy as np

src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "run.py")).read()
ns = {"np": np, "math": math}
want = {"quat_of", "mat_of", "slerp", "ground_plane"}
mod = ast.Module(body=[n for n in ast.parse(src).body if isinstance(n, ast.FunctionDef) and n.name in want], type_ignores=[])
exec(compile(mod, "run.py", "exec"), ns)
assert want <= ns.keys(), want - ns.keys()


def test_ground_plane_finds_a_tilted_floor_among_walls_and_noise():
    rs = np.random.default_rng(7)
    # a floor tilted 10 degrees about X, through (0, -2, 0); a wall at x = 5; scattered points
    t = math.radians(10)
    n_true = np.array([0, math.cos(t), math.sin(t)])
    u = np.array([1.0, 0, 0])
    v = np.cross(n_true, u)
    floor = np.array([0, -2.0, 0]) + rs.uniform(-10, 10, (600, 1)) * u + rs.uniform(-10, 10, (600, 1)) * v + rs.normal(0, 0.01, (600, 1)) * n_true
    wall = np.column_stack([np.full(400, 5.0), rs.uniform(-2, 6, 400), rs.uniform(-10, 10, 400)])
    noise = rs.uniform(-10, 10, (150, 3))
    pts = np.vstack([floor, wall, noise])
    n, cen, held = ground_plane(pts, np.array([0, 1.0, 0]), np.array([0, 1.6, 0]))  # noqa: F821
    assert abs(abs(np.dot(n, n_true)) - 1) < 1e-3, n
    assert np.dot(n, [0, 1, 0]) > 0, "the normal faces up, toward the camera"
    assert abs(np.dot(cen - np.array([0, -2.0, 0]), n_true)) < 0.02, cen
    assert 590 <= held <= 640, held


def test_ground_plane_turns_its_normal_toward_the_camera_and_gives_up_without_a_plane():
    rs = np.random.default_rng(3)
    floor = np.column_stack([rs.uniform(-5, 5, 200), np.zeros(200), rs.uniform(-5, 5, 200)])
    n, _, _ = ground_plane(floor, np.array([0, 1.0, 0]), np.array([0, -3.0, 0]))  # noqa: F821  (a camera under it)
    assert n[1] < -0.999
    assert ground_plane(floor[:20], np.array([0, 1.0, 0]), np.zeros(3)) is None  # noqa: F821
    walls = np.column_stack([np.full(200, 1.0), rs.uniform(-5, 5, 200), rs.uniform(-5, 5, 200)])
    assert ground_plane(walls, np.array([0, 1.0, 0]), np.zeros(3)) is None  # noqa: F821


def test_rotations_round_trip_and_slerp_halfway():
    rs = np.random.default_rng(5)
    for _ in range(50):
        q = rs.normal(size=4)
        q /= np.linalg.norm(q)
        R = mat_of(q)  # noqa: F821
        assert np.allclose(R @ R.T, np.eye(3), atol=1e-9)
        q2 = quat_of(R)  # noqa: F821
        assert min(np.linalg.norm(q2 - q), np.linalg.norm(q2 + q)) < 1e-9
    a = np.array([0, 0, 0, 1.0])
    b = np.array([0, math.sin(math.pi / 4), 0, math.cos(math.pi / 4)])  # 90 degrees about Y
    h = slerp(a, b, 0.5)  # noqa: F821
    assert np.allclose(h, [0, math.sin(math.pi / 8), 0, math.cos(math.pi / 8)])


if __name__ == "__main__":
    globals().update(ns)
    tests = [f for k, f in sorted(globals().items()) if k.startswith("test_")]
    for f in tests:
        f()
        print("ok", f.__name__)
    print(f"{len(tests)} passed")
