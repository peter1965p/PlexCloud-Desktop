<script setup lang="ts">
import { ref } from "vue";
import { api, home, type View } from "../api";

const props = defineProps<{ view: View }>();
const emit = defineEmits<{ changed: [] }>();
const remote = ref("");
const local = ref("");
const msg = ref("");
const editing = ref<string | null>(null);
const editPath = ref("");

const SYSTEM: Record<string, string> = { bilder: "XDG_PICTURES_DIR", videos: "XDG_VIDEOS_DIR", dokumente: "XDG_DOCUMENTS_DIR" };

function status(remoteName: string) {
  const m = props.view.status.mappings.find((x) => x.remote.toLowerCase() === remoteName.toLowerCase());
  switch (m?.state) {
    case "synchron": return { t: "synchron", c: "var(--ok)" };
    case "arbeitet": return { t: "arbeitet", c: "var(--accent)" };
    case "wartet": return { t: "wartet auf dich", c: "var(--accent)" };
    case "fehler": case "löschstopp": return { t: m.state, c: "var(--bad)" };
    default: return { t: "aus", c: "var(--muted)" };
  }
}

async function run(fn: () => Promise<unknown>) {
  msg.value = "";
  try {
    await fn();
    emit("changed");
    return true;
  } catch (e) {
    msg.value = String(e);
    return false;
  }
}

function suggest(name: string) {
  remote.value = name;
  local.value = `~/PlexCloud/${name}`;
}

async function add() {
  if (await run(() => api.setMapping(remote.value, local.value, true))) { remote.value = ""; local.value = ""; }
}
async function saveEdit(r: string, enabled: boolean) {
  if (await run(() => api.setMapping(r, editPath.value, enabled))) editing.value = null;
}
</script>

<template>
  <div>
    <h1 style="font-size: 26px">Wohin kommt was?</h1>
    <p class="muted" style="margin: 6px 0 0; max-width: 760px">
      Jeder Cloud-Ordner landet im passenden Systemordner, kein Sammelordner wie bei Drive oder OneDrive. Unterordner 1 : 1, Löschen heißt Papierkorb auf beiden Seiten.
    </p>
  </div>

  <div class="card">
    <div class="row head section-title"><span>PlexCloud</span><span /><span>Auf diesem Rechner</span><span>Aktiv</span><span style="text-align: right">Status</span></div>
    <div v-for="m in view.config.mappings" :key="m.remote" class="row">
      <span class="name">/{{ m.remote }}</span>
      <span class="muted mono">⇄</span>
      <span v-if="editing !== m.remote" class="mono small">
        {{ home(m.local) }}
        <span v-if="SYSTEM[m.remote.toLowerCase()]" class="muted"> · {{ SYSTEM[m.remote.toLowerCase()] }}</span>
        <a class="small" style="margin-left: 8px" @click="editing = m.remote; editPath = home(m.local)">ändern</a>
      </span>
      <span v-else class="edit">
        <input v-model="editPath" class="field mono" aria-label="Lokaler Ordner" @keydown.enter="saveEdit(m.remote, m.enabled)" />
        <button class="btn" @click="saveEdit(m.remote, m.enabled)">OK</button>
        <button class="btn ghost" @click="editing = null">Abbrechen</button>
      </span>
      <span><button class="switch" :class="{ on: m.enabled }" role="switch" :aria-checked="m.enabled" :aria-label="`/${m.remote} abgleichen`" @click="run(() => api.setMapping(m.remote, m.local, !m.enabled))" /></span>
      <span class="small" style="text-align: right" :style="{ color: status(m.remote).c }">
        {{ status(m.remote).t }}
        <a v-if="!['bilder', 'videos', 'dokumente'].includes(m.remote.toLowerCase())" class="small muted" style="margin-left: 8px" @click="run(() => api.removeMapping(m.remote))">entfernen</a>
      </span>
    </div>
    <div class="row add">
      <span class="name muted">+ Zuordnung</span>
      <span />
      <span class="edit">
        <input v-model="remote" class="field" placeholder="Cloud-Ordner, z. B. Musik" aria-label="Cloud-Ordner" style="max-width: 200px" />
        <input v-model="local" class="field mono" placeholder="~/Musik" aria-label="Lokaler Ordner" />
        <button class="btn" :disabled="!remote || !local" @click="add">Verbinden</button>
      </span>
      <span /><span />
    </div>
    <div v-if="view.status.unmapped.length" class="row hint small muted">
      <span style="grid-column: 1 / -1">
        Noch nicht verbunden:
        <a v-for="u in view.status.unmapped" :key="u" style="margin-right: 10px" @click="suggest(u)">/{{ u }}</a>
      </span>
    </div>
  </div>
  <p v-if="msg" class="small" style="color: var(--bad)" role="alert">{{ msg }}</p>

  <div class="facts">
    <div class="card fact"><b>Unterordner wie in der Cloud</b><span>/Bilder/2026 in der Cloud ist ~/Bilder/2026 auf dem Rechner. Struktur 1 : 1, nichts wird umsortiert.</span></div>
    <div class="card fact"><b>Echte Dateien, kein Platzhalter</b><span>Dolphin, GIMP und LibreOffice sehen normale Dateien.</span></div>
    <div class="card fact"><b>Gelöscht heißt Papierkorb</b><span>Lokal gelöscht: in der Cloud in den Papierkorb (30 Tage). In der Cloud gelöscht: lokal in den Papierkorb des Systems.</span></div>
  </div>
</template>

<style scoped>
.row { display: grid; grid-template-columns: 180px 30px minmax(0, 1fr) 70px 150px; align-items: center; gap: 8px; padding: 14px 18px; border-bottom: 1px solid var(--line); }
.row:last-child { border-bottom: 0; }
.row.head { padding: 10px 18px; }
.name { font-weight: 600; }
.edit { display: flex; gap: 8px; align-items: center; }
.facts { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 14px; }
.fact { padding: 16px; display: flex; flex-direction: column; gap: 6px; font-size: 13px; color: var(--text-2); }
.fact b { color: var(--text); font-size: 14px; }
</style>
