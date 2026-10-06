// #3383: a fan over async hooks overlaps their waits. `start` calls the hook
// and keeps its Promise in a slot; `wait`, the one Suspending import,
// settles every slot; `take` hands one settled value to the module, in arm
// order. The first start after a wait opens a new batch: a slot fan.any did
// not take was already awaited, and is dropped unread.
let fanSlots = [];
let fanOpen = false;
function fanStart(call) {
  if (!fanOpen) { fanSlots = []; fanOpen = true; }
  const slot = { ok: true, value: undefined, settled: null };
  let p;
  try { p = Promise.resolve(call()); } catch (e) { p = Promise.reject(e); }
  slot.settled = p.then((v) => { slot.value = v; }, (e) => { slot.ok = false; slot.value = e; });
  fanSlots.push(slot);
  return fanSlots.length - 1;
}
async function fanWait() {
  fanOpen = false;
  await Promise.all(fanSlots.map((s) => s.settled));
}
function fanTake(k) {
  const s = fanSlots[k];
  if (!s.ok) throw s.value;
  return s.value;
}
