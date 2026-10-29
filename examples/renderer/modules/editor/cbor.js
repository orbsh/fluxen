// 最小 CBOR (RFC 8949)——editor 封装模块专用：decode 覆盖宿主 ciborium 产出的
// 子集（uint/negint/bytes/text/array/map/tag(透明)/简单值/浮点），encode 覆盖
// 上行走的那一种形状（{code, lang} 这类 string→string 的 map，外加 null/bool/
// 数值/数组）。未知主类型一律抛错（fail-loud，对齐宿主惯例）；indefinite-length
// 不支持（宿主 ciborium 不发）。
export function decode(bytes) {
  const buf = bytes.buffer ?? bytes;
  const dv = new DataView(buf, bytes.byteOffset ?? 0, bytes.byteLength ?? bytes.length);
  let p = 0;

  function read() {
    const ib = dv.getUint8(p++);
    const mt = ib >> 5;
    const ai = ib & 31;

    if (mt === 7) {
      // simple / float
      if (ai < 24) return ai === 20 ? false : ai === 21 ? true : ai === 22 ? null : undefined;
      if (ai === 24) { const s = dv.getUint8(p++); return s === 20 ? false : s === 21 ? true : s === 22 ? null : undefined; }
      if (ai === 25) { const f = dv.getUint16(p); p += 2; return half(f); }
      if (ai === 26) { const f = dv.getFloat32(p); p += 4; return f; }
      if (ai === 27) { const f = dv.getFloat64(p); p += 8; return f; }
      throw new Error("cbor: bad simple ai " + ai);
    }

    let n;
    if (ai < 24) n = ai;
    else if (ai === 24) n = dv.getUint8(p++);
    else if (ai === 25) { n = dv.getUint16(p); p += 2; }
    else if (ai === 26) { n = dv.getUint32(p); p += 4; }
    else if (ai === 27) { n = Number(dv.getBigUint64(p)); p += 8; }
    else throw new Error("cbor: bad additional info " + ai);

    switch (mt) {
      case 0: return n;
      case 1: return -1 - n;
      case 2: { const b = new Uint8Array(n); for (let i = 0; i < n; i++) b[i] = dv.getUint8(p++); return b; }
      case 3: {
        const s = new Uint8Array(n);
        for (let i = 0; i < n; i++) s[i] = dv.getUint8(p++);
        return new TextDecoder().decode(s);
      }
      case 4: { const a = []; for (let i = 0; i < n; i++) a.push(read()); return a; }
      case 5: { const o = {}; for (let i = 0; i < n; i++) { const k = read(); o[k] = read(); } return o; }
      case 6: return read(); // tag: transparent wrapper
      default: throw new Error("cbor: unreachable");
    }
  }

  function half(u) {
    const e = (u >> 10) & 31, m = u & 1023;
    const v = e === 0 ? Math.pow(2, -24) * m : e === 31 ? (m ? NaN : Infinity) : Math.pow(2, e - 15) * (1 + m / 1024);
    return (u & 0x8000) ? -v : v;
  }

  return read();
}

/// 编码（上行走的那一种形状）：返回 Uint8Array，宿主按 CBOR 解回 JSON Value。
/// 源码可能很大——文本按字节循环写（不用展开参数，避免大数组爆栈）。
export function encode(v) {
  const out = [];
  const head = (mt, n) => {
    if (n < 24) out.push((mt << 5) | n);
    else if (n < 256) out.push((mt << 5) | 24, n);
    else if (n < 65536) out.push((mt << 5) | 25, (n >> 8) & 255, n & 255);
    else if (n < 4294967296) out.push((mt << 5) | 26, (n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255);
    else throw new Error("cbor: encode length too large");
  };
  const text = (s) => {
    const b = new TextEncoder().encode(s);
    head(3, b.length);
    for (let i = 0; i < b.length; i++) out.push(b[i]);
  };
  const walk = (x) => {
    if (x === null || x === undefined) return out.push(0xf6);
    if (x === true) return out.push(0xf5);
    if (x === false) return out.push(0xf4);
    if (typeof x === "string") return text(x);
    if (typeof x === "number") {
      if (Number.isInteger(x) && x >= 0) return head(0, x);
      if (Number.isInteger(x) && x < 0) return head(1, -1 - x);
      const b = new Uint8Array(8);
      new DataView(b.buffer).setFloat64(0, x);
      out.push(0xfb);
      for (let i = 0; i < 8; i++) out.push(b[i]);
      return;
    }
    if (Array.isArray(x)) {
      head(4, x.length);
      for (const i of x) walk(i);
      return;
    }
    if (typeof x === "object") {
      const ks = Object.keys(x).filter((k) => x[k] !== undefined);
      head(5, ks.length);
      for (const k of ks) { text(k); walk(x[k]); }
      return;
    }
    throw new Error("cbor: cannot encode " + typeof x);
  };
  walk(v);
  return new Uint8Array(out);
}