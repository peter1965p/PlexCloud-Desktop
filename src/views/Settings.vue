<script setup lang="ts">
import { ref, watch } from "vue";
import { api, applyAccent, SWATCHES, type View } from "../api";

const props = defineProps<{ view: View }>();
const emit = defineEmits<{ changed: [] }>();
const c = props.view.config;

const hex = ref(c.accent);
const hue = ref(20);
const limit = ref(c.upload_limit);
const excludes = ref(c.excludes.join(", "));
const autostart = ref(c.autostart);
const autoUpdate = ref(c.auto_update);
const msg = ref("");

watch(() => props.view.config.accent, (a) => (hex.value = a));

async function accent(h: string) {
  msg.value = "";
  try {
    await api.setAccent(h);
    applyAccent(h.toUpperCase());
    emit("changed");
  } catch (e) {
    msg.value = String(e);
  }
}

/** Farbton (0–360) → kräftige Farbe als Hex */
function fromHue(h: number) {
  const f = (n: number) => {
    const k = (n + h / 30) % 12;
    const v = 0.62 - 0.38 * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    return Math.round(v * 255).toString(16).padStart(2, "0");
  };
  return `#${f(0)}${f(8)}${f(4)}`.toUpperCase();
}

async function save() {
  msg.value = "";
  try {
    await api.saveSettings({ upload_limit: Math.max(0, Number(limit.value) || 0), excludes: excludes.value.split(","), autostart: autostart.value, auto_update: autoUpdate.value });
    emit("changed");
  } catch (e) {
    msg.value = String(e);
  }
}

async function logout() {
  await api.logout();
  emit("changed");
}
</script>

<template>
  <section class="card sec">
    <div class="sec-head">
      <div style="flex: 1">
        <h2>Akzentfarbe</h2>
        <div class="muted small">Knöpfe, Fortschritt, Tray-Symbol. Wie in der Handy-App frei wählbar.</div>
      </div>
      <span class="mono chip-val">{{ view.config.accent }}</span>
    </div>
    <div class="swatches">
      <button v-for="[name, h] in SWATCHES" :key="h" class="sw" :aria-label="name" :aria-pressed="view.config.accent === h" @click="accent(h)">
        <span class="dot-big" :style="{ background: h, boxShadow: view.config.accent === h ? `0 0 0 3px var(--surface), 0 0 0 5px ${h}` : 'none' }" />
        <span class="small">{{ name }}</span>
      </button>
      <div class="own">
        <label for="hex" class="small muted">Eigene Farbe (Hex oder Farbrad)</label>
        <div style="display: flex; gap: 8px">
          <input id="hex" v-model="hex" class="field mono" maxlength="7" @keydown.enter="accent(hex)" />
          <input type="color" :value="hex" aria-label="Farbe wählen" class="picker" @input="hex = ($event.target as HTMLInputElement).value.toUpperCase()" />
          <button class="btn" @click="accent(hex)">Übernehmen</button>
        </div>
        <input v-model.number="hue" type="range" min="0" max="359" class="hue" aria-label="Farbton" @input="hex = fromHue(hue)" @change="accent(fromHue(hue))" />
      </div>
    </div>
    <div class="preview">
      <span class="small muted">Vorschau</span>
      <button class="btn">Jetzt abgleichen</button>
      <div class="bar" style="width: 200px"><div style="width: 62%" /></div>
      <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 18a4.5 4.5 0 0 1-.6-8.96A6 6 0 0 1 18 8a4 4 0 0 1 0 10z" /></svg>
    </div>
  </section>

  <div class="two">
    <section class="card sec">
      <h2>Abgleich</h2>
      <div class="opt">
        <div style="flex: 1"><div>Beim Anmelden starten</div><div class="small muted">{{ view.platform === "windows" ? "Windows-Autostart" : "systemd-Benutzerdienst" }}</div></div>
        <button class="switch" :class="{ on: autostart }" role="switch" :aria-checked="autostart" aria-label="Beim Anmelden starten" @click="autostart = !autostart; save()" />
      </div>
      <div class="opt">
        <div style="flex: 1"><div>Upload-Grenze</div><div class="small muted">KB/s, 0 = unbegrenzt</div></div>
        <input v-model.number="limit" type="number" min="0" step="100" class="field mono" style="width: 110px" aria-label="Upload-Grenze in KB/s" @change="save" />
      </div>
      <div class="opt col">
        <div><div>Ausnahmen</div><div class="small muted">mit Komma getrennt; * als Platzhalter</div></div>
        <input v-model="excludes" class="field mono" aria-label="Ausnahmen" @change="save" />
      </div>
    </section>

    <section class="card sec">
      <h2>System</h2>
      <div class="opt">
        <div style="flex: 1"><div>Automatisch aktualisieren</div><div class="small muted">neue Versionen von GitHub selbst einspielen</div></div>
        <button class="switch" :class="{ on: autoUpdate }" role="switch" :aria-checked="autoUpdate" aria-label="Automatisch aktualisieren" @click="autoUpdate = !autoUpdate; save()" />
      </div>
      <div class="opt">
        <div style="flex: 1; min-width: 0"><div>Angemeldet</div><div class="small muted">{{ view.profile?.name || view.profile?.email }} · {{ view.profile?.provider }}</div></div>
        <button class="btn danger" @click="logout">Abmelden</button>
      </div>
      <div class="opt">
        <div style="flex: 1"><div>Version</div><a class="small" @click="api.web('https://github.com/peter1965p/PlexCloud-Desktop')">github.com/peter1965p/PlexCloud-Desktop</a></div>
        <span class="mono">{{ view.version }}</span>
      </div>
    </section>
  </div>
  <p v-if="msg" class="small" style="color: var(--bad)" role="alert">{{ msg }}</p>
</template>

<style scoped>
.sec { padding: 20px; display: flex; flex-direction: column; gap: 16px; }
.sec h2 { font-size: 18px; }
.sec-head { display: flex; align-items: center; gap: 12px; }
.chip-val { font-size: 13px; padding: 6px 10px; border-radius: 8px; background: var(--ground); border: 1px solid var(--border); }
.swatches { display: flex; gap: 14px; flex-wrap: wrap; align-items: flex-start; }
.sw { all: unset; cursor: pointer; display: flex; flex-direction: column; align-items: center; gap: 6px; width: 64px; color: var(--text-2); }
.sw:focus-visible .dot-big { outline: 2px solid var(--text); outline-offset: 6px; }
.dot-big { width: 44px; height: 44px; border-radius: 50%; }
.own { flex: 1; min-width: 280px; display: flex; flex-direction: column; gap: 10px; padding-left: 14px; border-left: 1px solid var(--line); }
.picker { width: 44px; height: 38px; padding: 2px; border-radius: 8px; border: 1px solid var(--border); background: var(--ground); cursor: pointer; }
.hue { -webkit-appearance: none; appearance: none; height: 10px; border-radius: 6px; background: linear-gradient(90deg, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00); }
.hue::-webkit-slider-thumb { -webkit-appearance: none; width: 18px; height: 18px; border-radius: 50%; background: var(--text); border: 3px solid var(--ground); cursor: pointer; }
.preview { display: flex; gap: 14px; align-items: center; padding-top: 14px; border-top: 1px solid var(--line); }
.two { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 18px; }
.opt { display: flex; align-items: center; gap: 12px; }
.opt.col { flex-direction: column; align-items: stretch; gap: 8px; }
</style>
