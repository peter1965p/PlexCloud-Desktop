import { invoke } from "@tauri-apps/api/core";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";

export interface Summary { uploads: number; upload_bytes: number; downloads: number; download_bytes: number; deletes: number; conflicts: number }
export interface MappingStatus { remote: string; local: string; enabled: boolean; approved: boolean; state: string; message: string; pending: Summary }
export interface Recent { name: string; dir: "up" | "down" | "trash" | "conflict"; folder: string; at: number }
export interface Current { name: string; dir: string; path: string; done: number; total: number }
export interface Status {
  signed_in: boolean; paused: boolean; running: boolean; online: boolean; error: string; last_sync: number
  mappings: MappingStatus[]; current: Current | null; recent: Recent[]
  usage: { used: number; limit: number | null; plan: string } | null; unmapped: string[]
}
export interface Mapping { remote: string; local: string; enabled: boolean; approved: boolean }
export interface Config { accent: string; mappings: Mapping[]; paused: boolean; upload_limit: number; excludes: string[]; autostart: boolean; auto_update: boolean; interval: number }
export interface Profile { name: string; email: string; picture: string | null; provider: string }
export interface View { status: Status; profile: Profile | null; config: Config; version: string; update: { version: string; url: string } | null }

export const api = {
  view: () => invoke<View>("view"),
  login: (provider: "google" | "email") => invoke<Profile>("login", { provider }),
  logout: () => invoke<void>("logout"),
  syncNow: () => invoke<void>("sync_now"),
  setPaused: (paused: boolean) => invoke<void>("set_paused", { paused }),
  approve: (remote: string) => invoke<void>("approve", { remote }),
  allowDeletes: (remote: string) => invoke<void>("allow_deletes", { remote }),
  setAccent: (accent: string) => invoke<void>("set_accent", { accent }),
  saveSettings: (s: { upload_limit: number; excludes: string[]; autostart: boolean; auto_update: boolean }) => invoke<void>("save_settings", { s }),
  setMapping: (remote: string, local: string, enabled: boolean) => invoke<void>("set_mapping", { remote, local, enabled }),
  removeMapping: (remote: string) => invoke<void>("remove_mapping", { remote }),
  open: (path: string) => openPath(path),
  web: (url: string) => openUrl(url),
};

const nf = new Intl.NumberFormat("de-DE", { maximumFractionDigits: 1 });
export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const u = ["KB", "MB", "GB", "TB"];
  let v = n / 1024, i = 0;
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return `${nf.format(v)} ${u[i]}`;
}

export function ago(sec: number): string {
  if (!sec) return "noch nie";
  const d = Math.max(0, Math.round(Date.now() / 1000 - sec));
  if (d < 60) return `vor ${d} s`;
  if (d < 3600) return `vor ${Math.round(d / 60)} min`;
  if (d < 86400) return `vor ${Math.round(d / 3600)} h`;
  return new Date(sec * 1000).toLocaleDateString("de-DE");
}

export function home(p: string): string {
  return p.replace(/^\/home\/[^/]+/, "~");
}

/** Akzentfarbe als CSS-Variablen setzen (inkl. hellerer Variante und Schrift auf dem Knopf) */
export function applyAccent(hex: string) {
  const r = document.documentElement.style;
  r.setProperty("--accent", hex);
  const v = parseInt(hex.slice(1), 16);
  const [R, G, B] = [(v >> 16) & 255, (v >> 8) & 255, v & 255];
  const lum = (0.2126 * R + 0.7152 * G + 0.0722 * B) / 255;
  r.setProperty("--on-accent", lum > 0.55 ? "#0A0F1E" : "#FFFFFF");
  r.setProperty("--accent-soft", `rgba(${R}, ${G}, ${B}, 0.14)`);
}

export const SWATCHES: [string, string][] = [["Orange", "#FF7A2F"], ["Blau", "#5B8CFF"], ["Türkis", "#3DD9C1"], ["Lila", "#A98BFF"], ["Grün", "#4CD964"], ["Pink", "#FF5C8A"]];
