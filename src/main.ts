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
const statusBar = document.querySelector<HTMLDivElement>("#status-bar")!;

const micSelect = document.querySelector<HTMLSelectElement>("#mic-device")!;
const micToggleBtn = document.querySelector<HTMLButtonElement>("#mic-toggle")!;
const micVolumeSlider = document.querySelector<HTMLInputElement>("#mic-volume")!;
const micVolumeValue = document.querySelector<HTMLSpanElement>("#mic-volume-value")!;
const masterVolumeSlider = document.querySelector<HTMLInputElement>("#master-volume")!;
const masterVolumeValue = document.querySelector<HTMLSpanElement>("#master-volume-value")!;

const hotkeyDisplay = document.querySelector<HTMLSpanElement>("#hotkey-display")!;
const hotkeySetBtn = document.querySelector<HTMLButtonElement>("#hotkey-set-btn")!;
const hotkeyClearBtn = document.querySelector<HTMLButtonElement>("#hotkey-clear-btn")!;

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
  statusBar.textContent = message;
  statusBar.classList.toggle("error", isError);
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

  for (const sound of sounds) {
    const card = document.createElement("div");
    card.className = "sound-card";
    card.textContent = sound.label;
    card.title = sound.path;
    card.addEventListener("click", () => playSound(sound));

    const removeBtn = document.createElement("button");
    removeBtn.className = "remove-btn";
    removeBtn.textContent = "×";
    removeBtn.title = "削除";
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
  micToggleBtn.textContent = enabled ? "マイク ON" : "マイク OFF";
}

async function toggleMic() {
  const next = !micEnabled;
  await invoke("set_mic_enabled", { enabled: next });
  setMicToggleUi(next);
  setStatus(next ? "マイクをONにしました" : "マイクをOFFにしました");
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
  hotkeyDisplay.classList.remove("recording");
  window.removeEventListener("keydown", onHotkeyKeydown);
  renderHotkey(currentShortcut);
}

async function onHotkeyKeydown(e: KeyboardEvent) {
  e.preventDefault();
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

function startHotkeyRecording() {
  if (recordingHotkey) return;
  recordingHotkey = true;
  hotkeyDisplay.textContent = "キーを押してください…";
  hotkeyDisplay.classList.add("recording");
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
hotkeySetBtn.addEventListener("click", startHotkeyRecording);
hotkeyClearBtn.addEventListener("click", clearHotkey);

listen<boolean>("mic-toggled", (event) => {
  setMicToggleUi(event.payload);
});

init();
