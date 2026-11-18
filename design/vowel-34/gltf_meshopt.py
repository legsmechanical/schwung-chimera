"""Minimal glTF/GLB loader with an EXT_meshopt_compression decoder written from the Khronos spec
(extensions/2.0/Vendor/EXT_meshopt_compression, Appendix A/B), and KHR_mesh_quantization
handled through the accessors' normalized flag. Enough to read three.js's facecap.glb.
"""
import json
import struct
import numpy as np


def _dezig8(v):
    return (~(v >> 1)) & 0xFF if (v & 1) else (v >> 1)


def decode_attributes(src, count, stride):
    assert src[0] == 0xA0, "attribute stream header"
    out = bytearray(count * stride)
    # the tail block is padded to 32 bytes and the baseline element is its LAST `stride` bytes
    # (the padding comes first: meshoptimizer reads data_end - vertex_size)
    last = bytearray(src[len(src) - stride:])
    pos = 1
    max_block = min((8192 // stride) & ~15, 256)
    done = 0
    while done < count:
        n = min(count - done, max_block)
        groups = (n + 15) // 16
        block = np.zeros((n, stride), dtype=np.uint8)
        for k in range(stride):
            hdr_len = (groups + 3) // 4
            hdr = src[pos: pos + hdr_len]
            pos += hdr_len
            deltas = []
            for gi in range(groups):
                bits = (hdr[gi // 4] >> ((gi % 4) * 2)) & 3
                if bits == 0:
                    d = [0] * 16
                elif bits == 3:
                    d = list(src[pos: pos + 16])
                    pos += 16
                else:
                    w = 2 if bits == 1 else 4
                    nb = 16 * w // 8
                    packed = src[pos: pos + nb]
                    pos += nb
                    vals = []
                    for byte in packed:
                        if w == 2:
                            vals += [(byte >> 6) & 3, (byte >> 4) & 3, (byte >> 2) & 3, byte & 3]
                        else:
                            vals += [(byte >> 4) & 15, byte & 15]
                    sentinel = (1 << w) - 1
                    d = []
                    for v in vals:
                        if v == sentinel:
                            d.append(src[pos])
                            pos += 1
                        else:
                            d.append(v)
                deltas += d
            # accumulate: each element's byte = previous + dezigzag(delta)
            prev = last[k]
            for e in range(n):
                prev = (prev + _dezig8(deltas[e])) & 0xFF
                block[e, k] = prev
            last[k] = prev
        out[done * stride: (done + n) * stride] = block.tobytes()
        done += n
    return bytes(out)


def _leb(src, pos):
    v, shift = 0, 0
    while True:
        b = src[pos]
        pos += 1
        v |= (b & 0x7F) << shift
        shift += 7
        if b < 0x80:
            return v, pos


def decode_triangles(src, count):
    assert src[0] == 0xE1, "triangle stream header"
    tri = count // 3
    codes = src[1: 1 + tri]
    dpos = 1 + tri
    aux = src[len(src) - 16:]
    nxt, last = 0, 0
    edge = [(0, 0)] * 16
    eo = 0
    vert = [0] * 16
    vo = 0
    out = []

    def push_e(a, b):
        nonlocal eo
        edge[eo % 16] = (a, b)
        eo += 1

    def push_v(v):
        nonlocal vo
        vert[vo % 16] = v
        vo += 1

    def get_e(i):
        return edge[(eo - 1 - i) % 16]

    def get_v(i):
        return vert[(vo - 1 - i) % 16]

    def dec_index():
        nonlocal dpos, last
        v, dpos = _leb(src, dpos)
        delta = ~(v >> 1) if (v & 1) else (v >> 1)
        last = (last + delta) & 0xFFFFFFFF
        return last

    for code in codes:
        x, y = code >> 4, code & 15
        if x < 15:
            a, b = get_e(x)
            if y == 0:
                c = nxt
                nxt += 1
                push_v(c)
            elif y < 13:
                c = get_v(y)
            elif y in (13, 14):
                c = (last - 1) if y == 13 else (last + 1)
                last = c
                push_v(c)
            else:
                c = dec_index()
                push_v(c)
            push_e(c, b)
            push_e(a, c)
        else:
            if y < 14:
                zw = aux[y]
                z, w = zw >> 4, zw & 15
                a = nxt
                nxt += 1
                if z == 0:
                    b = nxt
                    nxt += 1
                else:
                    b = get_v(z - 1)
                if w == 0:
                    c = nxt
                    nxt += 1
                else:
                    c = get_v(w - 1)
                push_e(b, a)
                push_e(c, b)
                push_e(a, c)
                push_v(a)
                if z == 0:
                    push_v(b)
                if w == 0:
                    push_v(c)
            else:
                zw = src[dpos]
                dpos += 1
                if zw == 0:
                    nxt = 0
                z, w = zw >> 4, zw & 15
                if y == 14:
                    a = nxt
                    nxt += 1
                else:
                    a = dec_index()
                if z == 0:
                    b = nxt
                    nxt += 1
                elif z < 15:
                    b = get_v(z - 1)
                else:
                    b = dec_index()
                if w == 0:
                    c = nxt
                    nxt += 1
                elif w < 15:
                    c = get_v(w - 1)
                else:
                    c = dec_index()
                push_e(b, a)
                push_e(c, b)
                push_e(a, c)
                push_v(a)
                if z == 0 or z == 15:
                    push_v(b)
                if w == 0 or w == 15:
                    push_v(c)
        out += [a, b, c]
    return out


def decode_indices(src, count):
    assert src[0] == 0xD1, "index stream header"
    pos = 1
    last = [0, 0]
    out = []
    for _ in range(count):
        v, pos = _leb(src, pos)
        base = v & 1
        delta = ~(v >> 2) if (v & 2) else (v >> 2)
        last[base] = (last[base] + delta) & 0xFFFFFFFF
        out.append(last[base])
    return out


def filter_octahedral(data, count, stride):
    if stride == 4:
        a = np.frombuffer(data, dtype=np.int8).reshape(count, 4).astype(np.float32)
        mx = 127.0
        dt = np.int8
    else:
        a = np.frombuffer(data, dtype=np.int16).reshape(count, 4).astype(np.float32)
        mx = 32767.0
        dt = np.int16
    one = a[:, 2]
    x, y = a[:, 0] / one, a[:, 1] / one
    z = 1.0 - np.abs(x) - np.abs(y)
    t = np.minimum(z, 0.0)
    x = x - np.copysign(t, x)
    y = y - np.copysign(t, y)
    ln = np.sqrt(x * x + y * y + z * z)
    o = np.stack([np.round(x / ln * mx), np.round(y / ln * mx), np.round(z / ln * mx), a[:, 3]], axis=1)
    return o.astype(dt).tobytes()


def filter_exponential(data, count, stride):
    a = np.frombuffer(data, dtype=np.int32).astype(np.int64)
    e = a >> 24
    m = (a << 40) >> 40  # sign-extend the low 24 bits
    return (np.power(2.0, e) * m).astype(np.float32).tobytes()


def filter_quaternion(data, count, stride):
    a = np.frombuffer(data, dtype=np.int16).reshape(count, 4).astype(np.int32)
    rng = 1.0 / np.sqrt(2.0)
    one = (a[:, 3] | 3).astype(np.float32)
    x, y, z = a[:, 0] / one * rng, a[:, 1] / one * rng, a[:, 2] / one * rng
    w = np.sqrt(np.maximum(0.0, 1.0 - x * x - y * y - z * z))
    mc = a[:, 3] & 3
    o = np.zeros((count, 4), dtype=np.float32)
    for i in range(count):
        c = mc[i]
        o[i, (c + 1) % 4] = x[i]
        o[i, (c + 2) % 4] = y[i]
        o[i, (c + 3) % 4] = z[i]
        o[i, c] = w[i]
    return np.round(o * 32767.0).astype(np.int16).tobytes()


class GLB:
    def __init__(self, path):
        b = open(path, "rb").read()
        jl = struct.unpack("<I", b[12:16])[0]
        self.j = json.loads(b[20: 20 + jl])
        off = 20 + jl
        bl = struct.unpack("<I", b[off: off + 4])[0]
        self.bin = b[off + 8: off + 8 + bl]
        self._views = {}

    def view(self, i):
        if i in self._views:
            return self._views[i]
        v = self.j["bufferViews"][i]
        ext = v.get("extensions", {}).get("EXT_meshopt_compression")
        if ext:
            src = self.bin[ext.get("byteOffset", 0): ext.get("byteOffset", 0) + ext["byteLength"]]
            mode, count, stride = ext["mode"], ext["count"], ext["byteStride"]
            if mode == "ATTRIBUTES":
                data = decode_attributes(src, count, stride)
                f = ext.get("filter", "NONE")
                if f == "OCTAHEDRAL":
                    data = filter_octahedral(data, count, stride)
                elif f == "EXPONENTIAL":
                    data = filter_exponential(data, count, stride)
                elif f == "QUATERNION":
                    data = filter_quaternion(data, count, stride)
            elif mode == "TRIANGLES":
                idx = decode_triangles(src, count)
                data = np.array(idx, dtype=np.uint16 if stride == 2 else np.uint32).tobytes()
            else:
                idx = decode_indices(src, count)
                data = np.array(idx, dtype=np.uint16 if stride == 2 else np.uint32).tobytes()
        else:
            o = v.get("byteOffset", 0)
            data = self.bin[o: o + v["byteLength"]]
        self._views[i] = data
        return data

    def accessor(self, i):
        a = self.j["accessors"][i]
        ncomp = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}[a["type"]]
        dt = {5120: np.int8, 5121: np.uint8, 5122: np.int16, 5123: np.uint16, 5125: np.uint32, 5126: np.float32}[a["componentType"]]
        data = self.view(a["bufferView"])
        bv = self.j["bufferViews"][a["bufferView"]]
        stride = bv.get("byteStride") or (np.dtype(dt).itemsize * ncomp)
        off = a.get("byteOffset", 0)
        n = a["count"]
        raw = np.frombuffer(data, dtype=np.uint8)
        rows = np.lib.stride_tricks.as_strided(raw[off:], shape=(n, np.dtype(dt).itemsize * ncomp), strides=(stride, 1))
        arr = np.frombuffer(rows.copy().tobytes(), dtype=dt).reshape(n, ncomp).astype(np.float64)
        if a.get("normalized"):
            mx = {np.int8: 127.0, np.uint8: 255.0, np.int16: 32767.0, np.uint16: 65535.0}[dt]
            arr = arr / mx
            if dt in (np.int8, np.int16):
                arr = np.maximum(arr, -1.0)
        return arr
