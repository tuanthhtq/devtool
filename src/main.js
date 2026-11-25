import { createIcon, hydrateIcons } from "./icons.js";
import {
  DEFAULT_SECRET_BYTES,
  generateApiToken,
  generatePassword,
  parseSecretByteLength,
} from "./secrets.js";

const invoke = window.__TAURI__?.core?.invoke;
const openDialog = window.__TAURI__?.dialog?.open;

const settingDefinitions = [
  { key: "startWithWindows", label: "Start Windows" },
  { key: "startMinimized", label: "Start minimized" },
];

const state = {
  avds: [],
  avdTooling: null,
  creatingAvd: false,
  creatingAvdBusy: false,
  apps: [],
  editingAppId: null,
  nginx: null,
  portSearchPromise: null,
  ports: [],
  toastTimer: null,
  settings: {
    startWithWindows: false,
    startMinimized: false,
    minimizeToTray: true,
    closeToTray: true,
    androidSdkPath: null,
    nginxPath: null,
    customApps: [],
  },
};

const elements = {};

async function callCommand(command, args = {}) {
  if (!invoke) {
    throw new Error("Tauri API is unavailable. Run this app with pnpm tauri dev.");
  }

  return invoke(command, args);
}

async function copyToClipboard(text) {
  if (invoke) {
    await callCommand("copy_to_clipboard", { text });
    return;
  }

  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }

  throw new Error("Clipboard is unavailable.");
}

function showToast(message, level = "info", { persist = false } = {}) {
  if (!elements.toast || !elements.toastMessage) {
    return;
  }

  window.clearTimeout(state.toastTimer);
  elements.toastMessage.textContent = message;
  elements.toast.dataset.level = level;
  elements.toast.setAttribute("role", level === "error" ? "alert" : "status");
  elements.toast.setAttribute("aria-live", level === "error" ? "assertive" : "polite");
  elements.toast.hidden = false;

  if (level !== "error" && !persist) {
    state.toastTimer = window.setTimeout(() => {
      elements.toast.hidden = true;
    }, 2200);
  }
}

function hideToast() {
  window.clearTimeout(state.toastTimer);
  if (elements.toast) {
    elements.toast.hidden = true;
  }
}

function setStatus(message) {
  showToast(message, "success");
}

function setError(error) {
  const message = error instanceof Error ? error.message : String(error);
  showToast(message, "error");
}

function createIconButton(label, variantClass, action, iconName, dataset = {}, options = {}) {
  const button = document.createElement("button");
  button.className = variantClass ? `icon-btn ${variantClass}` : "icon-btn";
  button.type = "button";
  button.dataset.action = action;
  button.setAttribute("aria-label", label);
  button.title = options.title || label;
  button.disabled = Boolean(options.disabled);

  for (const [key, value] of Object.entries(dataset)) {
    if (value !== undefined && value !== null) {
      button.dataset[key] = value;
    }
  }

  button.append(createIcon(iconName, 14));
  return button;
}

async function pickPath(options) {
  if (!openDialog) {
    throw new Error("Native file picker is unavailable.");
  }

  const selected = await openDialog(options);
  return Array.isArray(selected) ? selected[0] || null : selected;
}

function appNameFromPath(path) {
  const filename = path.split(/[\\/]/).pop() || "App";
  return filename.replace(/\.[^.]+$/, "") || filename;
}

function createEmptyRow(message) {
  const row = document.createElement("article");
  row.className = "list-row list-row-empty";

  const text = document.createElement("p");
  text.className = "status-text";
  text.textContent = message;

  row.append(text);
  return row;
}

function createAvdFormElement() {
  const tooling = state.avdTooling;
  const wrapper = document.createElement("div");
  wrapper.className = "avd-create-form";

  const top = document.createElement("div");
  top.className = "avd-create-top";

  if (tooling?.mode === "full") {
    const nameInput = document.createElement("input");
    nameInput.className = "avd-name-input";
    nameInput.type = "text";
    nameInput.placeholder = "AVD name (e.g. Pixel_Root)";
    nameInput.setAttribute("aria-label", "AVD name");
    nameInput.disabled = state.creatingAvdBusy;

    const select = document.createElement("select");
    select.className = "avd-select";
    select.setAttribute("aria-label", "Android image and architecture");
    select.disabled = state.creatingAvdBusy;

    const installed = tooling.images.filter((img) => img.installed);
    const downloadable = tooling.images.filter((img) => !img.installed);

    if (installed.length > 0) {
      const group = document.createElement("optgroup");
      group.label = "Installed";
      for (const img of installed) {
        const option = document.createElement("option");
        option.value = img.packageId;
        option.textContent = img.label;
        group.append(option);
      }
      select.append(group);
    }

    if (downloadable.length > 0) {
      const group = document.createElement("optgroup");
      group.label = "Download & Create";
      for (const img of downloadable) {
        const option = document.createElement("option");
        option.value = img.packageId;
        option.textContent = img.label;
        group.append(option);
      }
      select.append(group);
    }

    const actions = document.createElement("div");
    actions.className = "row-actions";
    actions.append(
      createIconButton(
        "Create AVD",
        "icon-btn-success",
        "submit-create-avd",
        "check",
        {},
        { disabled: state.creatingAvdBusy },
      ),
      createIconButton(
        "Cancel",
        "",
        "cancel-create-avd",
        "x",
        {},
        { disabled: state.creatingAvdBusy },
      ),
    );

    top.append(nameInput, actions);
    wrapper.append(top, select);
  } else {
    const select = document.createElement("select");
    select.className = "avd-select";
    select.setAttribute("aria-label", "Device profile");
    select.disabled = state.creatingAvdBusy;

    for (const profile of tooling?.profiles || ["medium_phone"]) {
      const option = document.createElement("option");
      option.value = profile;
      option.textContent = profile;
      select.append(option);
    }

    const actions = document.createElement("div");
    actions.className = "row-actions";
    actions.append(
      createIconButton(
        "Create AVD",
        "icon-btn-success",
        "submit-create-avd",
        "check",
        {},
        { disabled: state.creatingAvdBusy },
      ),
      createIconButton(
        "Cancel",
        "",
        "cancel-create-avd",
        "x",
        {},
        { disabled: state.creatingAvdBusy },
      ),
    );

    top.append(select, actions);
    wrapper.append(top);
  }

  return wrapper;
}

function renderAvds() {
  elements.avdList.replaceChildren();

  if (state.creatingAvd && state.avdTooling?.canCreate) {
    elements.avdList.append(createAvdFormElement());
  }

  if (state.avds.length === 0) {
    if (!state.creatingAvd) {
      elements.avdList.append(createEmptyRow("No AVDs found"));
    }
    return;
  }

  for (const avd of state.avds) {
    const isRunning = avd.status === "running";
    const row = document.createElement("article");
    row.className = "list-row";

    const main = document.createElement("div");
    main.className = "item-main";

    const dot = document.createElement("span");
    dot.className = isRunning ? "status-dot status-dot-running" : "status-dot";
    dot.setAttribute("aria-hidden", "true");

    const statusLabel = document.createElement("span");
    statusLabel.className = "sr-only";
    statusLabel.textContent = isRunning ? "Running: " : "Stopped: ";

    const title = document.createElement("p");
    title.className = "item-title";
    title.textContent = avd.name;
    title.title = `${avd.name} (${isRunning ? "running" : "stopped"})`;

    main.append(dot, statusLabel, title);

    const actions = document.createElement("div");
    actions.className = "row-actions";

    const runButton = isRunning
      ? createIconButton("Stop AVD", "icon-btn-danger", "stop-avd", "square", {
          name: avd.name,
          serial: avd.serial,
        })
      : createIconButton("Start AVD", "icon-btn-success", "start-avd", "play", {
          name: avd.name,
        });

    const deleteButton = createIconButton(
      "Delete AVD",
      "icon-btn-danger",
      "delete-avd",
      "trash",
      { name: avd.name },
      {
        disabled: isRunning,
        title: isRunning ? "Stop AVD before deleting" : "Delete AVD",
      },
    );

    actions.append(runButton, deleteButton);
    row.append(main, actions);
    elements.avdList.append(row);
  }
}

function focusCreateAvdField() {
  const input =
    elements.avdList.querySelector(".avd-name-input") ||
    elements.avdList.querySelector(".avd-select");
  input?.focus();
}

function renderApps() {
  elements.appsList.replaceChildren();

  if (state.apps.length === 0) {
    elements.appsList.append(createEmptyRow("No apps saved"));
    return;
  }

  for (const app of state.apps) {
    const row = document.createElement("article");
    row.className = "list-row app-row";

    const main = document.createElement("div");
    main.className = "item-main";

    const actions = document.createElement("div");
    actions.className = "row-actions";

    if (state.editingAppId === app.id) {
      const input = document.createElement("input");
      input.className = "app-name-input";
      input.type = "text";
      input.value = app.name;
      input.dataset.appId = app.id;
      input.setAttribute("aria-label", "Display name");
      main.append(input);

      actions.append(
        createIconButton("Save name", "icon-btn-success", "save-app-name", "check", {
          id: app.id,
        }),
        createIconButton("Cancel", "", "cancel-app-name", "x", { id: app.id }),
      );
    } else {
      const title = document.createElement("p");
      title.className = "item-title";
      title.textContent = app.name;
      title.title = app.name;
      main.append(title);

      actions.append(
        createIconButton("Run app", "icon-btn-success", "run-app", "play", { id: app.id }),
        createIconButton("Rename app", "", "edit-app", "pencil", { id: app.id }),
        createIconButton("Delete app", "icon-btn-danger", "delete-app", "trash", {
          id: app.id,
        }),
      );
    }

    row.append(main, actions);
    elements.appsList.append(row);
  }
}

function focusInlineAppName() {
  const input = elements.appsList.querySelector(".app-name-input");
  input?.focus();
  input?.select();
}

function renderNginx() {
  const path = state.nginx?.path || "Not configured";
  elements.nginxPath.textContent = path;
  elements.nginxPath.title = path;
}

function renderSettings() {
  elements.settingsGrid.replaceChildren();

  for (const setting of settingDefinitions) {
    const label = document.createElement("label");
    label.className = "checkbox-setting";

    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = Boolean(state.settings[setting.key]);
    input.dataset.action = "toggle-setting";
    input.dataset.key = setting.key;

    const text = document.createElement("span");
    text.textContent = setting.label;

    label.append(input, text);
    elements.settingsGrid.append(label);
  }
}

function renderPortResult() {
  elements.portResult.replaceChildren();

  if (state.ports.length === 0) {
    elements.portResult.append(createEmptyRow("No process found"));
    return;
  }

  for (const process of state.ports) {
    const row = document.createElement("article");
    row.className = "port-row";

    const stack = document.createElement("div");
    stack.className = "item-stack";

    const name = document.createElement("p");
    name.className = "process-name";
    name.textContent = process.processName;
    name.title = process.processName;

    const meta = document.createElement("p");
    meta.className = "process-meta";
    meta.textContent = `PID ${process.pid} · ${process.protocol} ${process.localAddress}`;

    stack.append(name, meta);
    row.append(
      stack,
      createIconButton("Kill process", "icon-btn-danger", "kill-process", "x", {
        pid: process.pid,
      }),
    );
    elements.portResult.append(row);
  }
}

async function refreshSettings() {
  state.settings = await callCommand("get_app_settings");
  renderSettings();
}

async function saveSettings() {
  state.settings = await callCommand("save_app_settings", { settings: state.settings });
  renderSettings();
}

async function syncTray() {
  try {
    await callCommand("refresh_tray");
  } catch {
    // Tray refresh is non-critical when main UI actions succeed.
  }
}

async function refreshAvds() {
  state.avds = await callCommand("scan_avds");
  renderAvds();
  await syncTray();
}

async function openCreateAvd() {
  if (state.creatingAvd) {
    state.creatingAvd = false;
    renderAvds();
    return;
  }

  showToast("Checking Android SDK tools...", "info", { persist: true });
  const tooling = await callCommand("get_avd_tooling_status");
  state.avdTooling = tooling;

  if (!tooling.canCreate) {
    throw new Error(tooling.missingReason || "Required Android SDK tools are missing.");
  }

  hideToast();
  state.creatingAvd = true;
  renderAvds();
  focusCreateAvdField();
}

async function submitCreateAvd() {
  const tooling = state.avdTooling;
  if (!tooling?.canCreate || state.creatingAvdBusy) {
    return;
  }

  let inputPayload;
  let needsDownload = false;

  if (tooling.mode === "full") {
    const nameInput = elements.avdList.querySelector(".avd-name-input");
    const select = elements.avdList.querySelector(".avd-select");
    const name = nameInput?.value.trim() || "";
    if (!name) {
      setError("AVD name is required");
      nameInput?.focus();
      return;
    }

    const packageId = select?.value;
    const selectedImg = tooling.images.find((img) => img.packageId === packageId);
    needsDownload = Boolean(selectedImg && !selectedImg.installed);
    if (
      needsDownload &&
      !window.confirm(`Download ${selectedImg.label.replace(" · Download", "")} and create '${name}'?`)
    ) {
      return;
    }

    inputPayload = { name, packageId, profile: null };
  } else {
    const select = elements.avdList.querySelector(".avd-select");
    inputPayload = { name: null, packageId: null, profile: select?.value || "medium_phone" };
  }

  state.creatingAvdBusy = true;
  renderAvds();
  showToast(
    needsDownload
      ? "Downloading system image & creating AVD (may take several minutes)..."
      : "Creating AVD...",
    "info",
    { persist: true },
  );

  try {
    const created = await callCommand("create_avd", { input: inputPayload });
    state.creatingAvd = false;
    state.avdTooling = null;
    await refreshAvds();
    setStatus(`Created AVD ${created}`);
  } finally {
    state.creatingAvdBusy = false;
    renderAvds();
  }
}

function cancelCreateAvd() {
  if (state.creatingAvdBusy) {
    return;
  }
  state.creatingAvd = false;
  renderAvds();
}

async function handleAvdDelete(name) {
  if (!name || !window.confirm(`Delete AVD '${name}'?`)) {
    return;
  }

  await callCommand("delete_avd", { name });
  await refreshAvds();
  setStatus(`Deleted ${name}`);
}

async function refreshApps() {
  state.apps = await callCommand("list_custom_apps");
  renderApps();
  await syncTray();
}

async function refreshNginx() {
  state.nginx = await callCommand("get_nginx_config");
  renderNginx();
  await syncTray();
}

async function refreshPorts({ afterPending = false } = {}) {
  if (state.portSearchPromise) {
    if (!afterPending) {
      return state.portSearchPromise;
    }

    try {
      await state.portSearchPromise;
    } catch {
      // A post-kill refresh should still run after a failed earlier lookup.
    }
  }

  const port = Number(elements.portInput.value || 3000);
  const searchPromise = callCommand("list_processes_by_port", { port });
  state.portSearchPromise = searchPromise;
  elements.portForm.setAttribute("aria-busy", "true");
  elements.portInput.disabled = true;
  elements.portForm.querySelector('[type="submit"]').disabled = true;

  try {
    state.ports = await searchPromise;
    renderPortResult();
  } finally {
    if (state.portSearchPromise === searchPromise) {
      state.portSearchPromise = null;
      elements.portForm.setAttribute("aria-busy", "false");
      elements.portInput.disabled = false;
      elements.portForm.querySelector('[type="submit"]').disabled = false;
    }
  }
}

async function loadApp() {
  try {
    await refreshSettings();
    await Promise.allSettled([refreshApps(), refreshNginx(), refreshAvds(), refreshPorts()]);
  } catch (error) {
    setError(error);
  }
}

async function saveCustomApp(app) {
  const saved = await callCommand("save_custom_app", { app });
  await refreshApps();
  return saved;
}

async function addApp() {
  const path = await pickPath({
    multiple: false,
    directory: false,
    filters: [
      {
        name: "Applications",
        extensions: ["exe", "bat", "cmd", "jar", "vbs"],
      },
    ],
  });
  if (!path) {
    return;
  }

  const saved = await saveCustomApp({
    id: null,
    name: appNameFromPath(path),
    path,
    args: null,
    cwd: null,
  });
  state.editingAppId = saved.id;
  renderApps();
  focusInlineAppName();
}

function startInlineAppNameEdit(app) {
  if (!app) {
    throw new Error("app not found");
  }

  state.editingAppId = app.id;
  renderApps();
  focusInlineAppName();
}

async function saveInlineAppName(app) {
  if (!app) {
    throw new Error("app not found");
  }

  const input = elements.appsList.querySelector(`.app-name-input[data-app-id="${app.id}"]`);
  const name = input?.value.trim();
  if (!name) {
    setError("Display name is required");
    input?.focus();
    return;
  }

  state.editingAppId = null;
  await saveCustomApp({ ...app, name });
  setStatus("App renamed");
}

function cancelInlineAppNameEdit() {
  state.editingAppId = null;
  renderApps();
}

async function handleAvdStart(name) {
  await callCommand("start_avd", { name });
  window.setTimeout(refreshAvds, 1200);
  setStatus(`Starting ${name}`);
}

async function handleAvdStop(name, serial) {
  if (!serial) {
    throw new Error(`No running emulator serial found for ${name}`);
  }

  if (!window.confirm(`Stop ${name}?`)) {
    return;
  }

  await callCommand("stop_avd", { serial });
  await refreshAvds();
  setStatus(`Stopped ${name}`);
}

async function handleSettingToggle(key) {
  state.settings = {
    ...state.settings,
    [key]: !state.settings[key],
  };

  if (key === "startWithWindows") {
    await callCommand("set_start_with_windows", { enabled: state.settings.startWithWindows });
  }

  await saveSettings();
  setStatus("Settings saved");
}

async function ensureNginxConfig() {
  if (state.nginx) {
    return state.nginx;
  }

  const path = await pickPath({
    multiple: false,
    directory: false,
    defaultPath: state.settings.nginxPath || undefined,
    filters: [{ name: "Nginx executable", extensions: ["exe"] }],
  });
  if (!path) {
    return null;
  }

  state.nginx = await callCommand("set_nginx_path", { path });
  state.settings = { ...state.settings, nginxPath: state.nginx.path };
  renderNginx();
  return state.nginx;
}

function readSecretByteLength() {
  const raw = elements.secretLength?.value ?? DEFAULT_SECRET_BYTES;
  return parseSecretByteLength(raw);
}

async function handleAction(event) {
  const button = event.target.closest("[data-action]");

  if (!button || button.disabled) {
    return;
  }

  const { action, id, key, name, serial, pid } = button.dataset;
  button.disabled = true;

  try {
    if (action === "open-create-avd") {
      await openCreateAvd();
    } else if (action === "submit-create-avd") {
      await submitCreateAvd();
    } else if (action === "cancel-create-avd") {
      cancelCreateAvd();
    } else if (action === "delete-avd") {
      await handleAvdDelete(name);
    } else if (action === "scan-avd") {
      await refreshAvds();
      setStatus("AVDs refreshed");
    } else if (action === "set-sdk") {
      const path = await pickPath({
        multiple: false,
        directory: true,
        defaultPath: state.settings.androidSdkPath || undefined,
      });
      if (path) {
        state.settings = { ...state.settings, androidSdkPath: path || null };
        state.avdTooling = null;
        await saveSettings();
        await refreshAvds();
        setStatus("Android SDK updated");
      }
    } else if (action === "start-avd") {
      await handleAvdStart(name);
    } else if (action === "stop-avd") {
      await handleAvdStop(name, serial);
    } else if (action === "add-app") {
      await addApp();
    } else if (action === "edit-app") {
      startInlineAppNameEdit(state.apps.find((app) => app.id === id));
    } else if (action === "save-app-name") {
      await saveInlineAppName(state.apps.find((app) => app.id === id));
    } else if (action === "cancel-app-name") {
      cancelInlineAppNameEdit();
    } else if (action === "delete-app") {
      const app = state.apps.find((candidate) => candidate.id === id);
      if (app && window.confirm(`Delete ${app.name}?`)) {
        await callCommand("delete_custom_app", { id });
        await refreshApps();
        setStatus("App deleted");
      }
    } else if (action === "run-app") {
      await callCommand("launch_custom_app", { id });
      setStatus("App launched");
    } else if (action === "nginx-start") {
      if (await ensureNginxConfig()) {
        const outcome = await callCommand("start_nginx");
        setStatus(outcome === "alreadyRunning" ? "Nginx already running" : "Nginx started");
      }
    } else if (action === "nginx-stop") {
      if ((await ensureNginxConfig()) && window.confirm("Stop nginx?")) {
        await callCommand("stop_nginx");
        setStatus("Nginx stopped");
      }
    } else if (action === "nginx-reload") {
      if (await ensureNginxConfig()) {
        await callCommand("reload_nginx");
        setStatus("Nginx reloaded");
      }
    } else if (action === "nginx-config") {
      if (await ensureNginxConfig()) {
        await callCommand("open_nginx_config");
        setStatus("Opened nginx.conf");
      }
    } else if (action === "toggle-setting") {
      await handleSettingToggle(key);
    } else if (action === "kill-process") {
      if (window.confirm(`Kill PID ${pid}?`)) {
        await callCommand("kill_process", { pid: Number(pid) });
        await refreshPorts({ afterPending: true });
        setStatus(`Killed PID ${pid}`);
      }
    } else if (action === "generate-api-token") {
      const bytes = readSecretByteLength();
      await copyToClipboard(generateApiToken(bytes));
      setStatus(`API token copied (${bytes}B)`);
    } else if (action === "generate-password") {
      const bytes = readSecretByteLength();
      await copyToClipboard(generatePassword(bytes));
      setStatus(`Password copied (${bytes}B)`);
    }
  } catch (error) {
    setError(error);
  } finally {
    if (button.isConnected && !state.creatingAvdBusy) {
      button.disabled = false;
    }
  }
}

async function handlePortSearch(event) {
  event.preventDefault();
  try {
    await refreshPorts();
  } catch (error) {
    setError(error);
  }
}

function handleGlobalKeydown(event) {
  if (event.key === "Escape" && elements.toast && !elements.toast.hidden) {
    hideToast();
    return;
  }

  if (event.target.closest(".avd-create-form")) {
    if (event.key === "Enter") {
      event.preventDefault();
      elements.avdList.querySelector('[data-action="submit-create-avd"]')?.click();
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelCreateAvd();
    }
    return;
  }

  if (!event.target.matches(".app-name-input")) {
    return;
  }

  if (event.key === "Enter") {
    event.preventDefault();
    event.target.closest(".app-row")?.querySelector('[data-action="save-app-name"]')?.click();
  } else if (event.key === "Escape") {
    event.preventDefault();
    cancelInlineAppNameEdit();
  }
}

function init() {
  hydrateIcons();

  elements.avdList = document.querySelector("#avd-list");
  elements.appsList = document.querySelector("#apps-list");
  elements.nginxPath = document.querySelector("#nginx-path");
  elements.portForm = document.querySelector("#port-form");
  elements.portInput = document.querySelector("#port-input");
  elements.portResult = document.querySelector("#port-result");
  elements.secretLength = document.querySelector("#secret-length");
  elements.settingsGrid = document.querySelector("#settings-grid");
  elements.toast = document.querySelector("#toast");
  elements.toastMessage = document.querySelector("#toast-message");
  elements.toastClose = document.querySelector("#toast-close");

  renderAvds();
  renderApps();
  renderNginx();
  renderSettings();
  renderPortResult();

  document.addEventListener("click", handleAction);
  document.addEventListener("keydown", handleGlobalKeydown);
  elements.portForm.addEventListener("submit", handlePortSearch);
  elements.toastClose.addEventListener("click", hideToast);
  loadApp();
}

window.addEventListener("DOMContentLoaded", init);
