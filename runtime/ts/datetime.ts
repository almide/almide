const __almd_datetime = {
  now(): number { return Math.floor(Date.now() / 1000); },
  from_parts(y: number, m: number, d: number, h: number, min: number, s: number): number { return Math.floor(Date.UTC(y, m - 1, d, h, min, s) / 1000); },
  parse_iso(s: string): number { const d = new Date(s); if (isNaN(d.getTime())) throw new Error(`invalid ISO 8601 datetime: ${s}`); return Math.floor(d.getTime() / 1000); },
  format(ts: number, pattern: string): string { const d = new Date(ts * 1000); const pad = (n: number, w: number = 2) => String(n).padStart(w, "0"); const Y = pad(d.getUTCFullYear(), 4); const m = pad(d.getUTCMonth() + 1); const dd = pad(d.getUTCDate()); const H = pad(d.getUTCHours()); const M = pad(d.getUTCMinutes()); const S = pad(d.getUTCSeconds()); const days = ["Mon","Tue","Wed","Thu","Fri","Sat","Sun"]; const months = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"]; const wd = d.getUTCDay(); const a = days[wd === 0 ? 6 : wd - 1]; const b = months[d.getUTCMonth()]; return pattern.replace("%F", `${Y}-${m}-${dd}`).replace("%T", `${H}:${M}:${S}`).replace("%Y", Y).replace("%m", m).replace("%d", dd).replace("%H", H).replace("%M", M).replace("%S", S).replace("%a", a).replace("%b", b); },
  to_iso(ts: number): string { const d = new Date(ts * 1000); const pad = (n: number, w: number = 2) => String(n).padStart(w, "0"); return `${pad(d.getUTCFullYear(), 4)}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}T${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}Z`; },
  year(ts: number): number { return new Date(ts * 1000).getUTCFullYear(); },
  month(ts: number): number { return new Date(ts * 1000).getUTCMonth() + 1; },
  day(ts: number): number { return new Date(ts * 1000).getUTCDate(); },
  hour(ts: number): number { return new Date(ts * 1000).getUTCHours(); },
  minute(ts: number): number { return new Date(ts * 1000).getUTCMinutes(); },
  second(ts: number): number { return new Date(ts * 1000).getUTCSeconds(); },
  weekday(ts: number): string { const days = ["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"]; return days[new Date(ts * 1000).getUTCDay()]; },
};
