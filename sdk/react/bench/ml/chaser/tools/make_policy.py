#!/usr/bin/env python3
"""The chaser's policy: a two-layer net written straight to ONNX (no onnx package: the protobuf is spelled out below).

    obs [1, 4] = (dx, dy, vx, vy)   the target's offset from the chaser and the chaser's velocity, roughly -1..1
    h = tanh(obs @ W1 + b1)         8 units: two that steer, two that damp, four with a little wander
    action [1, 2] = tanh(h @ W2 + b2)   the chaser's thrust in x and y

    python3 tools/make_policy.py media/policy.onnx
"""
import math, random, struct, sys


def varint(n):
    out = b""
    n &= (1 << 64) - 1
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out += bytes([b | 0x80])
        else:
            return out + bytes([b])


def field(num, wire, payload):
    return varint((num << 3) | wire) + payload


def i(num, v):
    return field(num, 0, varint(v))


def s(num, v):
    v = v if isinstance(v, bytes) else v.encode()
    return field(num, 2, varint(len(v)) + v)


def tensor(name, dims, values):
    return b"".join(i(1, d) for d in dims) + i(2, 1) + s(8, name) + s(9, struct.pack(f"<{len(values)}f", *values))


def value_info(name, dims):
    shape = b"".join(s(1, i(1, d)) for d in dims)
    return s(1, name) + s(2, s(1, i(1, 1) + s(2, shape)))


def node(op, inputs, outputs, name):
    return b"".join(s(1, x) for x in inputs) + b"".join(s(2, x) for x in outputs) + s(3, name) + s(4, op)


def main(out):
    r = random.Random(7)
    # W1 [4, 8]: rows are dx, dy, vx, vy
    W1 = [[0.0] * 8 for _ in range(4)]
    W1[0][0], W1[1][1] = 2.5, 2.5          # steer toward the target
    W1[2][2], W1[3][3] = -1.2, -1.2        # damp the velocity
    for u in range(4, 8):                  # wander
        for k in range(4):
            W1[k][u] = r.uniform(-0.6, 0.6)
    b1 = [0, 0, 0, 0] + [r.uniform(-0.3, 0.3) for _ in range(4)]
    W2 = [[0.0] * 2 for _ in range(8)]
    W2[0][0], W2[1][1] = 1.6, 1.6
    W2[2][0], W2[3][1] = 0.8, 0.8
    for u in range(4, 8):
        W2[u][0], W2[u][1] = r.uniform(-0.25, 0.25), r.uniform(-0.25, 0.25)
    b2 = [0.0, 0.0]
    nodes = [node("Gemm", ["obs", "W1", "b1"], ["z1"], "fc1"), node("Tanh", ["z1"], ["h"], "act1"), node("Gemm", ["h", "W2", "b2"], ["z2"], "fc2"), node("Tanh", ["z2"], ["action"], "act2")]
    inits = [tensor("W1", [4, 8], [x for row in W1 for x in row]), tensor("b1", [8], b1), tensor("W2", [8, 2], [x for row in W2 for x in row]), tensor("b2", [2], b2)]
    graph = b"".join(s(1, n) for n in nodes) + s(2, "chaser_policy") + b"".join(s(5, t) for t in inits) + s(11, value_info("obs", [1, 4])) + s(12, value_info("action", [1, 2]))
    model = i(1, 8) + s(2, "raea make_policy.py") + s(7, graph) + s(8, s(1, "") + i(2, 13))
    open(out, "wb").write(model)
    print(f"{out}: {len(model)} bytes")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "policy.onnx")
