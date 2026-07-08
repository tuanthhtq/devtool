const invoke = window.__TAURI__?.core?.invoke;
const openDialog = window.__TAURI__?.dialog?.open;

const settingDefinitions = [
  { key: "startWithWindows", label: "Start Windows" },
  { key: "startMinimized", label: "Start minimized" },
];

const state = {
  avds: [],
  apps: [],
  editingAppId: null,
  nginx: null,
  ports: [],
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

function writeLog(message, level = "info") {
  const time = new Date().toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
  const line = `[${time}] ${level === "error" ? "error" : ">"} ${message}`;
  const lines = [...elements.actionLog.textContent.split("\n"), line].slice(-50);

  elements.actionLog.textContent = lines.join("\n");
  elements.actionLog.dataset.level = level;
  elements.actionLog.parentElement.scrollTop = elements.actionLog.parentElement.scrollHeight;
}

function setStatus(message) {
  writeLog(message);
}

function setError(error) {
  const message = error instanceof Error ? error.message : String(error);
  writeLog(message, "error");
}

function createButton(label, className, action, dataset = {}, options = {}) {
  const button = document.createElement("button");
  button.className = options.emoji ? "emoji-action" : `btn ${className}`;
  button.type = "button";
  button.dataset.action = action;

  for (const [key, value] of Object.entries(dataset)) {
    if (value !== undefined && value !== null) {
      button.dataset[key] = value;
    }
  }

  if (options.emoji) {
    button.setAttribute("aria-label", label);
    button.title = label;
    button.textContent = options.emoji;
  } else {
    button.textContent = label;
  }

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
  row.className = "list-row";

  const text = document.createElement("p");
  text.className = "status-text";
  text.textContent = message;

  row.append(text);
  return row;
}

function renderAvds() {
  elements.avdList.replaceChildren();

  if (state.avds.length === 0) {
    elements.avdList.append(createEmptyRow("No AVDs found"));
    return;
  }

  for (const avd of state.avds) {
    const row = document.createElement("article");
    row.className = "list-row";

    const main = document.createElement("div");
    main.className = "item-main";

    const title = document.createElement("p");
    title.className = "item-title";
    title.textContent = avd.name;

    main.append(title);

    const actions = document.createElement("div");
    actions.className = "row-actions";

    const actionButton =
      avd.status === "running"
        ? createButton(
            "Stop",
            "btn-danger",
            "stop-avd",
            {
              name: avd.name,
              serial: avd.serial,
            },
            { emoji: "⏹️" },
          )
        : createButton(
            "Start",
            "btn-success",
            "start-avd",
            { name: avd.name },
            { emoji: "▶️" },
          );

    actions.append(actionButton);
    row.append(main, actions);
    elements.avdList.append(row);
  }
}

function renderApps() {
  elements.appsList.replaceChildren();

  if (state.apps.length === 0) {
    elements.appsList.append(createEmptyRow("No custom apps saved"));
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
        createButton(
          "Save display name",
          "btn-success",
          "save-app-name",
          { id: app.id },
          { emoji: "✅" },
        ),
        createButton(
          "Cancel",
          "btn-neutral",
          "cancel-app-name",
          { id: app.id },
          { emoji: "❌" },
        ),
      );
    } else {
      const title = document.createElement("p");
      title.className = "item-title";
      title.textContent = app.name;
      main.append(title);

      actions.append(
        createButton("Run", "btn-success", "run-app", { id: app.id }, { emoji: "▶️" }),
        createButton(
          "Edit",
          "btn-neutral",
          "edit-app",
          { id: app.id },
          { emoji: "✏️" },
        ),
        createButton(
          "Delete",
          "btn-danger",
          "delete-app",
          { id: app.id },
          { emoji: "🗑️" },
        ),
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
  elements.nginxPath.textContent = state.nginx?.path || "Not configured";
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

    const result = document.createElement("div");
    result.className = "item-main";

    const text = document.createElement("p");
    text.className = "process-text";
    text.textContent = `${process.processName}  PID ${process.pid}  ${process.protocol} ${process.localAddress}`;

    result.append(text);
    row.append(
      result,
      createButton("Kill", "btn-danger", "kill-process", { pid: process.pid }, { emoji: "❌" }),
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
  } catch (error) {
    writeLog(`tray sync skipped: ${error instanceof Error ? error.message : String(error)}`);
  }
}

async function refreshAvds() {
  state.avds = await callCommand("scan_avds");
  renderAvds();
  await syncTray();
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

async function refreshPorts() {
  const port = Number(elements.portInput.value || 3000);
  state.ports = await callCommand("list_processes_by_port", { port });
  renderPortResult();
}

async function loadApp() {
  setStatus("loading");
  try {
    await refreshSettings();
    await Promise.allSettled([refreshApps(), refreshNginx(), refreshAvds(), refreshPorts()]);
    setStatus("ready");
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
    setStatus("file selection cancelled");
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
  setStatus("app added - edit display name");
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
    setError("display name is required");
    input?.focus();
    return;
  }

  state.editingAppId = null;
  await saveCustomApp({ ...app, name });
  setStatus("display name saved");
}

function cancelInlineAppNameEdit() {
  state.editingAppId = null;
  renderApps();
  setStatus("display name edit cancelled");
}

async function handleAvdStart(name) {
  setStatus(`starting ${name}`);
  await callCommand("start_avd", { name });
  window.setTimeout(refreshAvds, 1200);
  setStatus(`${name} starting`);
}

async function handleAvdStop(name, serial) {
  if (!serial) {
    throw new Error(`No running emulator serial found for ${name}`);
  }

  if (!window.confirm(`Stop ${name}?`)) {
    return;
  }

  setStatus(`stopping ${name}`);
  await callCommand("stop_avd", { serial });
  await refreshAvds();
  setStatus(`${name} stopped`);
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
  setStatus("settings saved");
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
    setStatus("nginx selection cancelled");
    return null;
  }

  state.nginx = await callCommand("set_nginx_path", { path });
  state.settings = { ...state.settings, nginxPath: state.nginx.path };
  renderNginx();
  return state.nginx;
}

async function handleAction(event) {
  const button = event.target.closest("[data-action]");

  if (!button) {
    return;
  }

  const { action, id, key, name, serial, pid } = button.dataset;
  button.disabled = true;
  writeLog(`action: ${action}`);

  try {
    if (action === "scan-avd") {
      await refreshAvds();
      setStatus("AVDs scanned");
    } else if (action === "set-sdk") {
      const path = await pickPath({
        multiple: false,
        directory: true,
        defaultPath: state.settings.androidSdkPath || undefined,
      });
      if (path) {
        state.settings = { ...state.settings, androidSdkPath: path || null };
        await saveSettings();
        await refreshAvds();
        setStatus("Android SDK selected");
      } else {
        setStatus("SDK selection cancelled");
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
        setStatus("app deleted");
      }
    } else if (action === "run-app") {
      await callCommand("launch_custom_app", { id });
      setStatus("app launched");
    } else if (action === "nginx-start") {
      if (await ensureNginxConfig()) {
        await callCommand("start_nginx");
        setStatus("nginx started");
      }
    } else if (action === "nginx-stop") {
      if ((await ensureNginxConfig()) && window.confirm("Stop nginx?")) {
        await callCommand("stop_nginx");
        setStatus("nginx stopped");
      }
    } else if (action === "nginx-reload") {
      if (await ensureNginxConfig()) {
        await callCommand("reload_nginx");
        setStatus("nginx reloaded");
      }
    } else if (action === "nginx-config") {
      if (await ensureNginxConfig()) {
        await callCommand("open_nginx_config");
        setStatus("config opened");
      }
    } else if (action === "toggle-setting") {
      await handleSettingToggle(key);
    } else if (action === "kill-process") {
      if (window.confirm(`Kill PID ${pid}?`)) {
        await callCommand("kill_process", { pid: Number(pid) });
        await refreshPorts();
        setStatus("process killed");
      }
    }
  } catch (error) {
    setError(error);
  } finally {
    button.disabled = false;
  }
}

async function handlePortSearch(event) {
  event.preventDefault();
  setStatus("searching port");
  try {
    await refreshPorts();
    setStatus("port lookup complete");
  } catch (error) {
    setError(error);
  }
}

function handleAppNameKeydown(event) {
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
  elements.avdList = document.querySelector("#avd-list");
  elements.appsList = document.querySelector("#apps-list");
  elements.nginxPath = document.querySelector("#nginx-path");
  elements.portForm = document.querySelector("#port-form");
  elements.portInput = document.querySelector("#port-input");
  elements.portResult = document.querySelector("#port-result");
  elements.settingsGrid = document.querySelector("#settings-grid");
  elements.actionLog = document.querySelector("#action-log");

  renderAvds();
  renderApps();
  renderNginx();
  renderSettings();
  renderPortResult();

  document.addEventListener("click", handleAction);
  document.addEventListener("keydown", handleAppNameKeydown);
  elements.portForm.addEventListener("submit", handlePortSearch);
  loadApp();
}

window.addEventListener("DOMContentLoaded", init);
