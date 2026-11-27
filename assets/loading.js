// loading —— the stand-in for a region nothing has drawn into yet.
//
// ADR-0007 module contract: a `canvas` node names this file in `url`, the host calls
// mount(el, data, host, options) and keeps the returned id for update/resize/unmount. The module
// owns its render loop, so an animation needs nothing from the framework.
//
// It draws an SVG with SMIL (`animateTransform`): no CSS to ship beside it, no requestAnimation
// Frame, and it stops on its own when the node is removed. The payload's `size` (css length,
// default 32px) is the only knob; `host` is unused — a spinner has nothing to say to the venue.

const mounted = new Map();
let seq = 0;

function spinner() {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 50 50");
  svg.setAttribute("preserveAspectRatio", "xMidYMid meet");
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-label", "loading");

  const track = document.createElementNS(ns, "circle");
  track.setAttribute("cx", "25");
  track.setAttribute("cy", "25");
  track.setAttribute("r", "20");
  track.setAttribute("fill", "none");
  track.setAttribute("stroke", "currentColor");
  track.setAttribute("stroke-opacity", "0.15");
  track.setAttribute("stroke-width", "4");

  const arc = document.createElementNS(ns, "path");
  arc.setAttribute("d", "M45 25a20 20 0 0 0-20-20");
  arc.setAttribute("fill", "none");
  arc.setAttribute("stroke", "currentColor");
  arc.setAttribute("stroke-width", "4");
  arc.setAttribute("stroke-linecap", "round");
  const turn = document.createElementNS(ns, "animateTransform");
  turn.setAttribute("attributeName", "transform");
  turn.setAttribute("type", "rotate");
  turn.setAttribute("from", "0 25 25");
  turn.setAttribute("to", "360 25 25");
  turn.setAttribute("dur", "0.9s");
  turn.setAttribute("repeatCount", "indefinite");
  arc.appendChild(turn);

  svg.appendChild(track);
  svg.appendChild(arc);
  return svg;
}

export function mount(el, data, _host, options) {
  const cid = `loading-${++seq}`;
  // The host mounts this module only when the node carries a payload (ADR-0012), so `data` is the
  // knob and `options` the fallback: the venue's row says `{"size": "32px"}`.
  const size = (data && data.size) || (options && options.size) || "32px";

  const box = document.createElement("div");
  // The container is the whole region: the spinner sits in the middle of whatever was reserved for
  // the booth that has not drawn yet, and the region's own colour shows through (`currentColor`).
  box.style.cssText = "display:flex;align-items:center;justify-content:center;width:100%;height:100%;";
  const svg = spinner();
  svg.style.width = size;
  svg.style.height = size;
  box.appendChild(svg);
  el.appendChild(box);

  mounted.set(cid, box);
  return cid;
}

// Nothing arrives while nothing is drawn; both are here because the contract has four functions.
export function update(_cid, _cbor) {}
export function resize(_cid, _w, _h) {}

export function unmount(cid) {
  const box = mounted.get(cid);
  if (!box) return;
  box.remove();
  mounted.delete(cid);
}