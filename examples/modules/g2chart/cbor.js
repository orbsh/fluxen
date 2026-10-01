// 最小 CBOR (RFC 8949) 解码器——覆盖宿主 ciborium 产出的子集：
// uint/negint/bytes/text/array/map/tag(透明)/简单值(f,t,null)/浮点。
// 未知主类型抛错（fail-loud，对齐宿主惯例）。g2chart 封装专用，
// 不追求完备（indefinite-length 不支持——宿主 ciborium 不发）。
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

  const v = read();
  return v;
}
