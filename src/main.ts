// DESIGN.md の Copernicus / StyreneB はライセンス書体のため、
// 同文書が指定する代替 (Cormorant Garamond 500 / Inter / JetBrains Mono) を同梱する。
// Tauri はオフラインで動くため、CDN ではなくバンドルする必要がある。
import "@fontsource/cormorant-garamond/latin-500.css";
import "@fontsource/inter/latin-400.css";
import "@fontsource/inter/latin-500.css";
import "@fontsource/jetbrains-mono/latin-400.css";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

interface SoundEntry {
  id: string;
  label: string;
  path: string;
}

interface InitData {
  sounds: SoundEntry[];
  output_devices: string[];
  input_devices: string[];
  monitor_device: string | null;
  virtual_device: string | null;
  mic_device: string | null;
  mic_volume: number;
  master_volume: number;
  mic_toggle_shortcut: string | null;
}

const monitorSelect = document.querySelector<HTMLSelectElement>("#monitor-device")!;
const virtualSelect = document.querySelector<HTMLSelectElement>("#virtual-device")!;
const stopBtn = document.querySelector<HTMLButtonElement>("#stop-btn")!;
const addBtn = document.querySelector<HTMLButtonElement>("#add-btn")!;
const soundGrid = document.querySelector<HTMLDivElement>("#sound-grid")!;
const emptyState = document.querySelector<HTMLDivElement>("#empty-state")!;
const statusBar = document.querySelector<HTMLElement>("#status-bar")!;
const statusMessage = document.querySelector<HTMLSpanElement>("#status-message")!;
const soundCount = document.querySelector<HTMLSpanElement>("#sound-count")!;

const micSelect = document.querySelector<HTMLSelectElement>("#mic-device")!;
const micToggleBtn = document.querySelector<HTMLButtonElement>("#mic-toggle")!;
const micToggleText = micToggleBtn.querySelector<HTMLSpanElement>(".live-btn-text")!;
const micVolumeSlider = document.querySelector<HTMLInputElement>("#mic-volume")!;
const micVolumeValue = document.querySelector<HTMLSpanElement>("#mic-volume-value")!;
const masterVolumeSlider = document.querySelector<HTMLInputElement>("#master-volume")!;
const masterVolumeValue = document.querySelector<HTMLSpanElement>("#master-volume-value")!;

const hotkeyDisplay = document.querySelector<HTMLButtonElement>("#hotkey-display")!;
const hotkeyClearBtn = document.querySelector<HTMLButtonElement>("#hotkey-clear-btn")!;

const themeToggle = document.querySelector<HTMLButtonElement>("#theme-toggle")!;
const themeToggleLabel = document.querySelector<HTMLSpanElement>("#theme-toggle-label")!;

const tabs = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="tab"]'));
// サウンドタブでのみ意味を持つ要素（操作ボタンと件数表示）
const soundsOnly = Array.from(document.querySelectorAll<HTMLElement>(".sounds-action"));

const THEME_KEY = "soundboard-theme";
type Theme = "dark" | "light";

// ブラウザのKeyboardEvent.codeでは、修飾キー自体を押した瞬間にもkeydownが発火する。
// これらはメインキーとして扱わず、修飾キーが揃うまで待つ。
const MODIFIER_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "ShiftLeft",
  "ShiftRight",
  "MetaLeft",
  "MetaRight",
]);

let sounds: SoundEntry[] = [];
// マイクのON/OFFは意図せぬ集音を避けるため、起動時は常にOFFから始める。
let micEnabled = false;
let currentShortcut: string | null = null;
let recordingHotkey = false;

function setStatus(message: string, isError = false) {
  statusMessage.textContent = message;
  statusBar.classList.toggle("is-error", isError);
}

// WAI-ARIA のタブパターン: aria-selected と roving tabindex を同期させる
function selectTab(target: HTMLButtonElement, focus = false) {
  for (const tab of tabs) {
    const selected = tab === target;
    tab.setAttribute("aria-selected", String(selected));
    tab.tabIndex = selected ? 0 : -1;

    const panelId = tab.getAttribute("aria-controls")!;
    document.getElementById(panelId)!.hidden = !selected;
  }

  const onSounds = target.id === "tab-sounds";
  for (const el of soundsOnly) el.hidden = !onSounds;
  soundCount.hidden = !onSounds;

  if (focus) target.focus();
}

function onTabKeydown(e: KeyboardEvent) {
  const index = tabs.indexOf(e.currentTarget as HTMLButtonElement);
  let next: number | null = null;

  if (e.key === "ArrowRight") next = (index + 1) % tabs.length;
  else if (e.key === "ArrowLeft") next = (index - 1 + tabs.length) % tabs.length;
  else if (e.key === "Home") next = 0;
  else if (e.key === "End") next = tabs.length - 1;

  if (next !== null) {
    e.preventDefault();
    selectTab(tabs[next], true);
  }
}

function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  // ボタンは押したときに何が起きるかを示す（現在のテーマ名ではなく切り替え先）
  const next = theme === "dark" ? "light" : "dark";
  themeToggleLabel.textContent = next.toUpperCase();
  themeToggle.title = next === "dark" ? "ダークテーマに切り替える" : "ライトテーマに切り替える";
}

function currentTheme(): Theme {
  return document.documentElement.dataset.theme === "dark" ? "dark" : "light";
}

function toggleTheme() {
  const next: Theme = currentTheme() === "dark" ? "light" : "dark";
  localStorage.setItem(THEME_KEY, next);
  applyTheme(next);
}

function initTheme() {
  const saved = localStorage.getItem(THEME_KEY);
  if (saved === "dark" || saved === "light") {
    applyTheme(saved);
    return;
  }
  // 既定は cream canvas のライト（DESIGN.md の基調）。OSがダーク指定なら従う。
  const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  applyTheme(prefersDark ? "dark" : "light");
}

function populateDeviceSelect(select: HTMLSelectElement, devices: string[], selected: string | null) {
  select.innerHTML = "";
  const noneOption = document.createElement("option");
  noneOption.value = "";
  noneOption.textContent = "（未選択）";
  select.appendChild(noneOption);

  for (const device of devices) {
    const option = document.createElement("option");
    option.value = device;
    option.textContent = device;
    select.appendChild(option);
  }

  select.value = selected ?? "";
}

function renderSounds() {
  soundGrid.innerHTML = "";
  emptyState.hidden = sounds.length > 0;
  soundCount.textContent = `${sounds.length} 件`;

  for (const sound of sounds) {
    // 削除ボタンを内包するため button 要素は使えない。role と keydown で
    // キーボード操作性を確保する。
    const card = document.createElement("div");
    card.className = "sound-card";
    card.textContent = sound.label;
    card.title = sound.path;
    card.setAttribute("role", "button");
    card.tabIndex = 0;
    card.addEventListener("click", () => playSound(sound));
    card.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        playSound(sound);
      }
    });

    const removeBtn = document.createElement("button");
    removeBtn.className = "remove-btn";
    removeBtn.type = "button";
    removeBtn.textContent = "×";
    removeBtn.title = `${sound.label} を削除`;
    removeBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      removeSound(sound.id);
    });

    card.appendChild(removeBtn);
    soundGrid.appendChild(card);
  }
}

async function playSound(sound: SoundEntry) {
  try {
    await invoke("play_sound", { path: sound.path });
    setStatus(`再生: ${sound.label}`);
  } catch (err) {
    setStatus(`再生に失敗しました: ${err}`, true);
  }
}

async function removeSound(id: string) {
  try {
    await invoke("remove_sound", { id });
    sounds = sounds.filter((s) => s.id !== id);
    renderSounds();
  } catch (err) {
    setStatus(`削除に失敗しました: ${err}`, true);
  }
}

async function addSound() {
  const selected = await open({
    multiple: false,
    filters: [{ name: "音声ファイル", extensions: ["wav", "mp3", "ogg", "flac", "opus"] }],
  });

  if (!selected || Array.isArray(selected)) {
    return;
  }

  const fileName = selected.split(/[\\/]/).pop() ?? selected;
  const defaultLabel = fileName.replace(/\.[^.]+$/, "");
  const label = window.prompt("ボタンに表示する名前を入力してください", defaultLabel);
  if (label === null) {
    return;
  }

  try {
    const entry = await invoke<SoundEntry>("add_sound", {
      path: selected,
      label: label.trim() || defaultLabel,
    });
    sounds.push(entry);
    renderSounds();
    setStatus(`追加しました: ${entry.label}`);
  } catch (err) {
    setStatus(`追加に失敗しました: ${err}`, true);
  }
}

async function stopAll() {
  await invoke("stop_all");
  setStatus("停止しました");
}

async function updateDevices() {
  const monitor = monitorSelect.value || null;
  const virtualDevice = virtualSelect.value || null;
  try {
    await invoke("set_devices", { monitor, virtualDevice });
    setStatus("出力デバイスを更新しました");
  } catch (err) {
    setStatus(`出力デバイスの設定に失敗しました: ${err}`, true);
  }
}

function setMicToggleUi(enabled: boolean) {
  micEnabled = enabled;
  micToggleBtn.setAttribute("aria-pressed", String(enabled));
  micToggleText.textContent = enabled ? "マイク送信中" : "マイク停止中";
}

async function toggleMic() {
  const next = !micEnabled;
  await invoke("set_mic_enabled", { enabled: next });
  setMicToggleUi(next);
  setStatus(next ? "マイクを送信しています" : "マイクを停止しました");
}

async function updateMicDevice() {
  const device = micSelect.value || null;
  try {
    await invoke("set_mic_device", { device });
    setStatus("マイクデバイスを更新しました");
  } catch (err) {
    setStatus(`マイクデバイスの設定に失敗しました: ${err}`, true);
  }
}

async function updateMicVolume() {
  const volume = Number(micVolumeSlider.value) / 100;
  micVolumeValue.textContent = `${micVolumeSlider.value}%`;
  await invoke("set_mic_volume", { volume });
}

async function updateMasterVolume() {
  const volume = Number(masterVolumeSlider.value) / 100;
  masterVolumeValue.textContent = `${masterVolumeSlider.value}%`;
  await invoke("set_master_volume", { volume });
}

// "Ctrl+Alt+KeyM" のような内部表現を "Ctrl+Alt+M" のような表示用文字列に変換する。
function formatShortcut(shortcut: string): string {
  return shortcut
    .split("+")
    .map((token) => token.replace(/^Key/, "").replace(/^Digit/, ""))
    .join("+");
}

function renderHotkey(shortcut: string | null) {
  currentShortcut = shortcut;
  hotkeyDisplay.textContent = shortcut ? formatShortcut(shortcut) : "未設定";
  hotkeyClearBtn.hidden = !shortcut;
}

function stopHotkeyRecording() {
  recordingHotkey = false;
  hotkeyDisplay.classList.remove("is-recording");
  window.removeEventListener("keydown", onHotkeyKeydown);
  renderHotkey(currentShortcut);
}

async function onHotkeyKeydown(e: KeyboardEvent) {
  e.preventDefault();
  if (e.code === "Escape" && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) {
    stopHotkeyRecording();
    setStatus("ホットキーの登録をやめました");
    return;
  }
  if (MODIFIER_CODES.has(e.code)) {
    return; // 修飾キー単体はメインキーとして扱わず待ち続ける
  }

  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");

  if (mods.length === 0) {
    setStatus("Ctrl / Alt / Shift のいずれかを含めてください", true);
    return; // 修飾キーなしは登録せず録音継続
  }

  const shortcut = [...mods, e.code].join("+");
  stopHotkeyRecording();

  try {
    await invoke("set_mic_toggle_shortcut", { shortcut });
    renderHotkey(shortcut);
    setStatus(`ホットキーを設定しました: ${formatShortcut(shortcut)}`);
  } catch (err) {
    setStatus(`ホットキーの設定に失敗しました: ${err}`, true);
  }
}

// 表示欄クリックで録音開始。録音中にもう一度クリックするとキャンセル。
function toggleHotkeyRecording() {
  if (recordingHotkey) {
    stopHotkeyRecording();
    return;
  }
  recordingHotkey = true;
  hotkeyDisplay.textContent = "キーを押す（Escで中止）";
  hotkeyDisplay.classList.add("is-recording");
  window.addEventListener("keydown", onHotkeyKeydown);
}

async function clearHotkey() {
  try {
    await invoke("set_mic_toggle_shortcut", { shortcut: null });
    renderHotkey(null);
    setStatus("ホットキーを解除しました");
  } catch (err) {
    setStatus(`ホットキーの解除に失敗しました: ${err}`, true);
  }
}

async function init() {
  try {
    const data = await invoke<InitData>("get_init_data");
    sounds = data.sounds;
    populateDeviceSelect(monitorSelect, data.output_devices, data.monitor_device);
    populateDeviceSelect(virtualSelect, data.output_devices, data.virtual_device);
    populateDeviceSelect(micSelect, data.input_devices, data.mic_device);
    setMicToggleUi(false);

    const micVolumePercent = Math.round(data.mic_volume * 100);
    micVolumeSlider.value = String(micVolumePercent);
    micVolumeValue.textContent = `${micVolumePercent}%`;

    const masterVolumePercent = Math.round(data.master_volume * 100);
    masterVolumeSlider.value = String(masterVolumePercent);
    masterVolumeValue.textContent = `${masterVolumePercent}%`;

    renderHotkey(data.mic_toggle_shortcut);
    renderSounds();
    setStatus("準備完了");
  } catch (err) {
    setStatus(`初期化に失敗しました: ${err}`, true);
  }
}

addBtn.addEventListener("click", addSound);
stopBtn.addEventListener("click", stopAll);
monitorSelect.addEventListener("change", updateDevices);
virtualSelect.addEventListener("change", updateDevices);
micToggleBtn.addEventListener("click", toggleMic);
micSelect.addEventListener("change", updateMicDevice);
micVolumeSlider.addEventListener("input", updateMicVolume);
masterVolumeSlider.addEventListener("input", updateMasterVolume);
hotkeyDisplay.addEventListener("click", toggleHotkeyRecording);
hotkeyClearBtn.addEventListener("click", clearHotkey);
themeToggle.addEventListener("click", toggleTheme);

for (const tab of tabs) {
  tab.addEventListener("click", () => selectTab(tab));
  tab.addEventListener("keydown", onTabKeydown);
}

listen<boolean>("mic-toggled", (event) => {
  setMicToggleUi(event.payload);
});

initTheme();
init();
