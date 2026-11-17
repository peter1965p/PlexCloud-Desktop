<script setup lang="ts">
import { ref } from "vue";
import { api } from "../api";

defineProps<{ error: string }>();
const windows = navigator.userAgent.includes("Windows");
const emit = defineEmits<{ done: [] }>();
const busy = ref<"" | "google" | "email">("");
const msg = ref("");

async function go(p: "google" | "email") {
  busy.value = p;
  msg.value = "";
  try {
    await api.login(p);
    emit("done");
  } catch (e) {
    msg.value = String(e);
  } finally {
    busy.value = "";
  }
}
</script>

<template>
  <div class="wrap">
    <div class="card box">
      <svg width="44" height="44" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 18a4.5 4.5 0 0 1-.6-8.96A6 6 0 0 1 18 8a4 4 0 0 1 0 10z" /></svg>
      <h1>PlexCloud für {{ windows ? "Windows" : "Linux" }}</h1>
      <p class="muted">Bilder, Videos und Dokumente landen in deinen echten Ordnern {{ windows ? "Bilder, Videos und Dokumente" : "~/Bilder, ~/Videos und ~/Dokumente" }}. Kein Sammelordner.</p>
      <button class="btn big" :disabled="!!busy" @click="go('google')">
        <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M21.6 12.2c0-.7-.1-1.4-.2-2H12v3.8h5.4a4.6 4.6 0 0 1-2 3v2.5h3.2c1.9-1.7 3-4.3 3-7.3zM12 22c2.7 0 5-.9 6.6-2.4l-3.2-2.5c-.9.6-2 1-3.4 1-2.6 0-4.8-1.8-5.6-4.1H3.1v2.6A10 10 0 0 0 12 22zm-5.6-8a6 6 0 0 1 0-3.9V7.5H3.1a10 10 0 0 0 0 9zM12 5.9c1.5 0 2.8.5 3.8 1.5l2.9-2.9A10 10 0 0 0 3.1 7.5l3.3 2.6C7.2 7.7 9.4 5.9 12 5.9z" /></svg>
        {{ busy === "google" ? "Browser ist offen …" : "Mit Google anmelden" }}
      </button>
      <button class="btn ghost big" :disabled="!!busy" @click="go('email')">{{ busy === "email" ? "Browser ist offen …" : "Mit E-Mail anmelden" }}</button>
      <p v-if="busy" class="muted small">Melde dich im Browser an. Danach geht es hier automatisch weiter.</p>
      <p v-if="msg || error" class="err small" role="alert">{{ msg || error }}</p>
    </div>
  </div>
</template>

<style scoped>
.wrap { height: 100%; display: grid; place-items: center; padding: 24px; }
.box { width: 420px; max-width: 100%; padding: 32px; display: flex; flex-direction: column; gap: 14px; align-items: flex-start; }
h1 { font-size: 24px; }
p { margin: 0; }
.big { width: 100%; height: 42px; justify-content: center; }
.err { color: var(--bad); }
</style>
