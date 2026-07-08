import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

interface SoundEntry {
  id: string;
  label: string;
  path: string;
}

interface InitData {
  sounds: SoundEntry[];
  devices: string[];
  monitor_device: string | null;
  virtual_device: string | null;
}

const monitorSelect = document.querySelector<HTMLSelectElement>("#monitor-device")!;
const virtualSelect = document.querySelector<HTMLSelectElement>("#virtual-device")!;
const stopBtn = document.querySelector<HTMLButtonElement>("#stop-btn")!;
const addBtn = document.querySelector<HTMLButtonElement>("#add-btn")!;
const soundGrid = document.querySelector<HTMLDivElement>("#sound-grid")!;
const emptyState = document.querySelector<HTMLDivElement>("#empty-state")!;
const statusBar = document.querySelector<HTMLDivElement>("#status-bar")!;

let sounds: SoundEntry[] = [];

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

async function init() {
  try {
    const data = await invoke<InitData>("get_init_data");
    sounds = data.sounds;
    populateDeviceSelect(monitorSelect, data.devices, data.monitor_device);
    populateDeviceSelect(virtualSelect, data.devices, data.virtual_device);
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

init();
