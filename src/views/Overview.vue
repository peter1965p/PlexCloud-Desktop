<script setup lang="ts">
import { computed } from "vue";
import { ago, api, bytes, home, type MappingStatus, type View } from "../api";

const props = defineProps<{ view: View }>();
const emit = defineEmits<{ changed: []; goto: ["mappings" | "settings"] }>();
const s = computed(() => props.view.status);

const headline = computed(() => {
  if (s.value.paused) return "Pausiert";
  if (s.value.current) return s.value.current.dir === "up" ? "Lädt hoch …" : "Lädt herunter …";
  if (s.value.running) return "Gleicht ab …";
  if (s.value.error) return s.value.online ? "Problem beim Abgleich" : "Keine Verbindung";
  if (s.value.mappings.some((m) => m.state === "wartet")) return "Erster Abgleich wartet";
  if (s.value.mappings.some((m) => m.state === "löschstopp" || m.state === "fehler")) return "Bitte kurz ansehen";
  return "Alles synchron";
});
const active = computed(() => s.value.mappings.filter((m) => m.enabled));
const pct = computed(() => (s.value.current && s.value.current.total ? Math.round((s.value.current.done / s.value.current.total) * 100) : 0));

const ICONS: Record<string, { color: string; path: string }> = {
  bilder: { color: "var(--blue)", path: '<rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-5-5L5 21"/>' },
  videos: { color: "var(--violet)", path: '<rect x="2" y="5" width="15" height="14" rx="2"/><path d="m17 10 5-3v10l-5-3"/>' },
  dokumente: { color: "var(--folder)", path: '<path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><path d="M14 3v6h6M8 13h8M8 17h5"/>' },
};
const icon = (r: string) => ICONS[r.toLowerCase()] ?? { color: "var(--muted)", path: '<path d="M3 7h6l2 2h10v10H3z"/>' };

function stateLabel(m: MappingStatus) {
  if (s.value.current && s.value.running && m.state === "arbeitet") return { text: s.value.current.dir === "up" ? `↑ ${pct.value} %` : `↓ ${pct.value} %`, color: "var(--accent)" };
  switch (m.state) {
    case "synchron": return { text: "synchron", color: "var(--ok)" };
    case "wartet": return { text: "wartet auf dich", color: "var(--accent)" };
    case "löschstopp": return { text: "Löschstopp", color: "var(--bad)" };
    case "fehler": return { text: "Fehler", color: "var(--bad)" };
    case "aus": return { text: "aus", color: "var(--muted)" };
    default: return { text: "prüft …", color: "var(--muted)" };
  }
}

function plan(m: MappingStatus) {
  const p = m.pending, out: string[] = [];
  if (p.uploads) out.push(`${p.uploads} hochladen (${bytes(p.upload_bytes)})`);
  if (p.downloads) out.push(`${p.downloads} herunterladen (${bytes(p.download_bytes)})`);
  if (p.conflicts) out.push(`${p.conflicts} Konflikte (beide Fassungen bleiben)`);
  if (p.deletes) out.push(`${p.deletes} in den Papierkorb`);
  return out.length ? out.join(" · ") : "Beide Seiten sind schon gleich.";
}

async function act(fn: () => Promise<unknown>) {
  await fn();
  emit("changed");
}

const DIRS: Record<string, { sign: string; color: string; label: string }> = {
  up: { sign: "↑", color: "var(--accent)", label: "hochgeladen" },
  down: { sign: "↓", color: "var(--blue)", label: "heruntergeladen" },
  trash: { sign: "✕", color: "var(--muted)", label: "Papierkorb" },
  conflict: { sign: "!", color: "var(--bad)", label: "Konfliktkopie, beide Fassungen bleiben" },
};
</script>

<template>
  <div class="head">
    <div style="flex: 1">
      <h1>{{ headline }}</h1>
      <div class="muted small mono">
        letzter Abgleich {{ ago(s.last_sync) }} · beobachtet {{ active.length }} Ordner
        <span v-if="s.error" style="color: var(--bad)"> · {{ s.error }}</span>
      </div>
    </div>
    <button class="btn ghost" @click="act(() => api.setPaused(!s.paused))">{{ s.paused ? "Fortsetzen" : "Pausieren" }}</button>
    <button class="btn" :disabled="s.paused || s.running" @click="act(api.syncNow)">Jetzt abgleichen</button>
  </div>

  <!-- erster Abgleich, Löschstopp, Fehler: hier entscheiden -->
  <div v-for="m in s.mappings.filter((x) => x.enabled && ['wartet', 'löschstopp', 'fehler'].includes(x.state))" :key="'a' + m.remote" class="card alert" :class="m.state">
    <div style="flex: 1">
      <div class="alert-title">
        <template v-if="m.state === 'wartet'">Erster Abgleich: /{{ m.remote }} ⇄ {{ home(m.local) }}</template>
        <template v-else-if="m.state === 'löschstopp'">Löschstopp bei /{{ m.remote }}</template>
        <template v-else>Problem bei /{{ m.remote }}</template>
      </div>
      <div class="small muted">{{ m.state === "wartet" ? plan(m) : m.message }}</div>
      <div v-if="m.state === 'wartet'" class="small muted">Gleich heißende Dateien mit gleicher Größe werden nicht doppelt übertragen. Gelöscht wird beim ersten Mal nichts.</div>
    </div>
    <button v-if="m.state === 'wartet'" class="btn" @click="act(() => api.approve(m.remote))">Abgleich starten</button>
    <template v-else-if="m.state === 'löschstopp'">
      <button class="btn ghost" @click="api.open(m.local)">Ordner ansehen</button>
      <button class="btn danger" @click="act(() => api.allowDeletes(m.remote))">Ja, löschen</button>
    </template>
  </div>

  <div class="grid">
    <div v-for="m in active" :key="m.remote" class="card tile" role="button" tabindex="0" @click="api.open(m.local)" @keydown.enter="api.open(m.local)">
      <div class="tile-head">
        <div class="ico"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" :stroke="icon(m.remote).color" stroke-width="2" aria-hidden="true" v-html="icon(m.remote).path" /></div>
        <div style="flex: 1; min-width: 0">
          <div class="tile-name">{{ m.remote }}</div>
        </div>
        <span class="small" :style="{ color: stateLabel(m).color }">{{ stateLabel(m).text }}</span>
      </div>
      <div class="paths mono small">
        <div><span class="muted">☁</span> /{{ m.remote }}</div>
        <div><span class="muted">⌂</span> {{ home(m.local) }}</div>
      </div>
    </div>
    <button class="card tile add" @click="emit('goto', 'mappings')">
      <span class="muted">+ Weiteren Ordner verbinden</span>
      <span v-if="s.unmapped.length" class="small muted">In der Cloud: {{ s.unmapped.slice(0, 4).join(", ") }}</span>
    </button>
  </div>

  <div class="card activity">
    <div class="act-head">
      <span class="section-title">Aktivität</span>
    </div>
    <div v-if="s.current" class="row">
      <span class="sign" :style="{ color: s.current.dir === 'up' ? 'var(--accent)' : 'var(--blue)' }">{{ s.current.dir === "up" ? "↑" : "↓" }}</span>
      <div style="flex: 1; min-width: 0">
        <div class="ellipsis">{{ s.current.name }}</div>
        <div class="bar" style="margin-top: 6px"><div :style="{ width: pct + '%' }" /></div>
      </div>
      <span class="mono small muted right">{{ bytes(s.current.done) }} / {{ bytes(s.current.total) }}<br />{{ home(s.current.path) }}</span>
    </div>
    <div v-for="r in s.recent" :key="r.at + r.name + r.dir" class="row">
      <span class="sign" :style="{ color: DIRS[r.dir]?.color }">{{ DIRS[r.dir]?.sign }}</span>
      <div class="ellipsis" style="flex: 1">{{ r.name }} <span class="muted small">· {{ DIRS[r.dir]?.label }}</span></div>
      <span class="mono small muted right">{{ home(r.folder) }} · {{ ago(r.at) }}</span>
    </div>
    <div v-if="!s.current && !s.recent.length" class="row muted small">Noch nichts übertragen in dieser Sitzung.</div>
  </div>
</template>

<style scoped>
.head { display: flex; align-items: center; gap: 10px; }
.head h1 { font-size: 26px; }
.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: 14px; }
.tile { padding: 16px; display: flex; flex-direction: column; gap: 12px; cursor: pointer; text-align: left; color: inherit; font: inherit; }
.tile:hover { border-color: var(--muted); }
.tile-head { display: flex; align-items: center; gap: 12px; }
.ico { width: 36px; height: 36px; border-radius: 10px; background: var(--ground); border: 1px solid var(--border); display: grid; place-items: center; }
.tile-name { font-weight: 600; font-size: 15px; }
.paths { display: flex; flex-direction: column; gap: 3px; color: var(--text-2); }
.add { border-style: dashed; background: transparent; justify-content: center; align-items: center; min-height: 112px; gap: 6px; }
.activity { padding: 6px 0; }
.act-head { padding: 10px 18px; }
.row { display: flex; align-items: center; gap: 12px; padding: 10px 18px; border-top: 1px solid var(--line); font-size: 13px; }
.sign { font-family: var(--mono); width: 14px; text-align: center; }
.right { text-align: right; }
.ellipsis { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.alert { padding: 16px 18px; display: flex; align-items: center; gap: 12px; border-color: var(--accent); background: linear-gradient(0deg, var(--accent-soft), var(--accent-soft)), var(--surface); }
.alert.löschstopp, .alert.fehler { border-color: rgba(255, 138, 138, .5); background: var(--surface); }
.alert-title { font-weight: 600; margin-bottom: 2px; }
</style>
