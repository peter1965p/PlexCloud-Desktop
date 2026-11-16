<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { api, applyAccent, bytes, type View } from "./api";
import Login from "./views/Login.vue";
import Overview from "./views/Overview.vue";
import Mappings from "./views/Mappings.vue";
import Settings from "./views/Settings.vue";

const view = ref<View | null>(null);
const tab = ref<"overview" | "mappings" | "settings">("overview");
let timer: number | undefined;
const updating = ref(false);
async function doUpdate() {
  updating.value = true;
  try { await api.installUpdate(); } catch (e) { updating.value = false; alert(String(e)); }
}

async function refresh() {
  try { view.value = await api.view(); } catch { /* Backend startet noch */ }
}

onMounted(() => {
  refresh();
  // nur solange das Fenster sichtbar ist
  timer = window.setInterval(() => { if (!document.hidden) refresh(); }, 1000);
});
onUnmounted(() => window.clearInterval(timer));

watch(() => view.value?.config.accent, (a) => a && applyAccent(a), { immediate: true });

const initials = computed(() => {
  const p = view.value?.profile;
  const n = (p?.name || p?.email?.split("@")[0] || "?").split(/[\s._-]+/).filter(Boolean);
  return n.slice(0, 2).map((x) => x[0]!.toUpperCase()).join("") || "?";
});
const waiting = computed(() => view.value?.status.mappings.filter((m) => m.state === "wartet" || m.state === "löschstopp" || m.state === "fehler").length ?? 0);
const usage = computed(() => view.value?.status.usage);
</script>

<template>
  <div v-if="!view" class="boot muted">PlexCloud startet …</div>
  <Login v-else-if="!view.status.signed_in" :error="view.status.error" @done="refresh" />
  <div v-else class="shell">
    <nav class="nav" aria-label="Bereiche">
      <div class="brand">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 18a4.5 4.5 0 0 1-.6-8.96A6 6 0 0 1 18 8a4 4 0 0 1 0 10z" /></svg>
        <span>PlexCloud</span>
      </div>
      <button class="navlink" :class="{ on: tab === 'overview' }" @click="tab = 'overview'">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M3 12h4l3-8 4 16 3-8h4" /></svg>Übersicht
        <span v-if="waiting" class="badge">{{ waiting }}</span>
      </button>
      <button class="navlink" :class="{ on: tab === 'mappings' }" @click="tab = 'mappings'">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M3 7h6l2 2h10v10H3z" /></svg>Ordner-Zuordnung
      </button>
      <button class="navlink" :class="{ on: tab === 'settings' }" @click="tab = 'settings'">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><circle cx="12" cy="12" r="3" /><path d="M12 2v3M12 19v3M4.2 4.2l2.1 2.1M17.7 17.7l2.1 2.1M2 12h3M19 12h3M4.2 19.8l2.1-2.1M17.7 6.3l2.1-2.1" /></svg>Einstellungen
      </button>
      <div style="flex: 1" />
      <div v-if="usage" class="usage">
        <div class="section-title">Speicher</div>
        <div class="mono">{{ bytes(usage.used) }} <span class="muted">· {{ usage.limit ? "von " + bytes(usage.limit) : "unbegrenzt" }}</span></div>
        <div class="bar"><div :style="{ width: usage.limit ? Math.min(100, (usage.used / usage.limit) * 100) + '%' : '8%' }" /></div>
      </div>
      <div v-if="view.profile" class="me">
        <img v-if="view.profile.picture" :src="view.profile.picture" alt="" referrerpolicy="no-referrer" />
        <div v-else class="avatar">{{ initials }}</div>
        <div style="min-width: 0">
          <div class="ellipsis">{{ view.profile.name || view.profile.email }}</div>
          <div class="muted small ellipsis">{{ view.profile.provider }}</div>
        </div>
      </div>
    </nav>
    <main class="main">
      <div v-if="view.update" class="update">
        <span style="flex: 1">Neue Version {{ view.update.version }} ist da.{{ updating ? " Wird eingespielt, PlexCloud startet gleich neu …" : "" }}</span>
        <a @click="api.web(view.update.url)">Was ist neu</a>
        <a v-if="view.update.self_update && !updating" @click="doUpdate">Jetzt aktualisieren</a>
      </div>
      <Overview v-if="tab === 'overview'" :view="view" @changed="refresh" @goto="tab = $event" />
      <Mappings v-else-if="tab === 'mappings'" :view="view" @changed="refresh" />
      <Settings v-else :view="view" @changed="refresh" />
    </main>
  </div>
</template>

<style scoped>
.boot { height: 100%; display: grid; place-items: center; }
.shell { display: flex; height: 100%; }
.nav { width: 230px; flex: none; background: var(--nav); border-right: 1px solid var(--line); padding: 16px 12px; display: flex; flex-direction: column; gap: 4px; }
.brand { display: flex; align-items: center; gap: 10px; font: 700 16px var(--display); padding: 4px 10px 16px; }
.navlink { all: unset; display: flex; align-items: center; gap: 10px; padding: 9px 10px; border-radius: 8px; color: var(--text-2); cursor: pointer; font-size: 14px; }
.navlink:hover { background: rgba(255, 255, 255, .03); }
.navlink.on { background: var(--accent-soft); color: var(--text); }
.navlink.on svg { color: var(--accent); }
.badge { margin-left: auto; background: var(--accent); color: var(--on-accent); font-size: 11px; font-weight: 700; border-radius: 10px; padding: 1px 7px; }
.usage { padding: 12px 10px; display: flex; flex-direction: column; gap: 6px; font-size: 13px; }
.me { display: flex; align-items: center; gap: 10px; padding: 12px 10px 4px; border-top: 1px solid var(--line); font-size: 13px; }
.me img, .avatar { width: 34px; height: 34px; border-radius: 50%; flex: none; border: 2px solid var(--accent); object-fit: cover; }
.avatar { display: grid; place-items: center; background: var(--surface); font-weight: 700; font-size: 12px; }
.ellipsis { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.main { flex: 1; min-width: 0; overflow-y: auto; padding: 24px 32px; display: flex; flex-direction: column; gap: 18px; }
.update { background: var(--accent-soft); border: 1px solid var(--accent); border-radius: 10px; padding: 10px 14px; font-size: 13px; display: flex; gap: 10px; }
</style>
