const SVG_NS = "http://www.w3.org/2000/svg";

const ICON_PATHS = {
  key: [
    '<path d="m15.5 7.5 2.3 2.3a1 1 0 0 0 1.4 0l2.1-2.1a1 1 0 0 0 0-1.4L19 4"/>',
    '<path d="m21 2-9.6 9.6"/>',
    '<circle cx="7.5" cy="15.5" r="5.5"/>',
  ],
  lock: [
    '<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/>',
    '<path d="M7 11V7a5 5 0 0 1 10 0v4"/>',
  ],
  search: [
    '<circle cx="11" cy="11" r="8"/>',
    '<path d="m21 21-4.3-4.3"/>',
  ],
  plus: [
    '<path d="M5 12h14"/>',
    '<path d="M12 5v14"/>',
  ],
  folder: [
    '<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>',
  ],
  play: ['<polygon points="6 3 20 12 6 21 6 3"/>'],
  square: ['<rect width="14" height="14" x="5" y="5" rx="2"/>'],
  refresh: [
    '<path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/>',
    '<path d="M3 3v5h5"/>',
    '<path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16"/>',
    '<path d="M16 16h5v5"/>',
  ],
  file: [
    '<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/>',
    '<path d="M14 2v4a2 2 0 0 0 2 2h4"/>',
    '<path d="M10 9H8"/>',
    '<path d="M16 13H8"/>',
    '<path d="M16 17H8"/>',
  ],
  pencil: [
    '<path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"/>',
  ],
  trash: [
    '<path d="M3 6h18"/>',
    '<path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/>',
    '<path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>',
  ],
  check: ['<path d="M20 6 9 17l-5-5"/>'],
  x: [
    '<path d="M18 6 6 18"/>',
    '<path d="m6 6 12 12"/>',
  ],
  hash: [
    '<line x1="4" x2="20" y1="9" y2="9"/>',
    '<line x1="4" x2="20" y1="15" y2="15"/>',
    '<line x1="10" x2="8" y1="3" y2="21"/>',
    '<line x1="16" x2="14" y1="3" y2="21"/>',
  ],
  eye: [
    '<path d="M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0"/>',
    '<circle cx="12" cy="12" r="3"/>',
  ],
  eyeOff: [
    '<path d="M10.733 5.076a10.744 10.744 0 0 1 11.205 6.575 1 1 0 0 1 0 .696 10.747 10.747 0 0 1-1.444 2.49"/>',
    '<path d="M14.084 14.158a3 3 0 0 1-4.242-4.242"/>',
    '<path d="M17.479 17.499a10.75 10.75 0 0 1-15.417-5.151 1 1 0 0 1 0-.696 10.75 10.75 0 0 1 4.446-5.143"/>',
    '<path d="m2 2 20 20"/>',
  ],
};

export function createIcon(name, size = 14) {
  const paths = ICON_PATHS[name];
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "2");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("icon");

  if (paths) {
    svg.innerHTML = paths.join("");
  }

  return svg;
}

export function hydrateIcons(root = document) {
  for (const placeholder of root.querySelectorAll("[data-icon]")) {
    const name = placeholder.dataset.icon;
    const size = Number(placeholder.dataset.iconSize) || 14;
    placeholder.replaceWith(createIcon(name, size));
  }
}
