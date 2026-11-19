// Minimal stroke icons (24×24, drawn for Kahie).
const P = {
  back: 'M15 5l-7 7 7 7',
  pen: 'M4 20l1.5-5L16 4.5a2.1 2.1 0 013 3L8.5 18 4 20zM14 6.5l3 3',
  highlighter: 'M9 15l-4 4h5l2-2M9 15l7-10 4 3-7 10-4-3z',
  eraser: 'M8 20h12M5.5 15.5l9-9a2 2 0 012.8 0l1.7 1.7a2 2 0 010 2.8L11 19H8.5l-3-3a1 1 0 010-.5zM10 11l5 5',
  select: 'M5 4l13 6-5.5 1.8L10.5 18 5 4z',
  text: 'M5 6V4h14v2M12 4v16M9 20h6',
  math: 'M4 12h3l3 7 4-15h6M14 13l5 5M19 13l-5 5',
  plot: 'M4 4v16h16M7 15c3-9 5-9 7-4s3 4 5-3',
  image: 'M4 5h16v14H4zM4 16l5-5 4 4 2-2 5 5M15 9.5a1.5 1.5 0 100-.01',
  shapes: 'M3 20l6-10 6 10H3zM14 4h7v7h-7zM17 17a3.5 3.5 0 107 0 3.5 3.5 0 00-7 0',
  hand: 'M8 13V6.5a1.5 1.5 0 013 0V12M11 11V4.5a1.5 1.5 0 013 0V12M14 11V6.5a1.5 1.5 0 013 0V14c0 4-2.5 7-6 7-2.5 0-4-1-5.5-3L3.6 14.3a1.5 1.5 0 012.3-1.9L8 15',
  undo: 'M9 14L4 9l5-5M4 9h10.5a5.5 5.5 0 010 11H11',
  redo: 'M15 14l5-5-5-5M20 9H9.5a5.5 5.5 0 000 11H13',
  more: 'M5 12h.01M12 12h.01M19 12h.01',
  plus: 'M12 5v14M5 12h14',
  prev: 'M15 6l-6 6 6 6',
  next: 'M9 6l6 6-6 6',
  search: 'M11 18a7 7 0 100-14 7 7 0 000 14zM20 20l-4-4',
  trash: 'M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3',
  copy: 'M9 9h11v11H9zM5 15H4V4h11v1',
  edit: 'M4 20h4L19 9l-4-4L4 16v4zM13.5 6.5l4 4',
  fit: 'M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5',
  download: 'M12 4v11M7 10l5 5 5-5M5 20h14',
  upload: 'M12 20V9M7 14l5-5 5 5M5 4h14',
  book: 'M5 4h11a3 3 0 013 3v13H8a3 3 0 01-3-3V4zM5 17a3 3 0 013-3h11',
  sparkle: 'M12 3l2 6 6 2-6 2-2 6-2-6-6-2 6-2 2-6z',
  close: 'M6 6l12 12M18 6L6 18',
  check: 'M5 12l5 5 9-10',
  wand: 'M4 20L16 8M14 4v3M18 8h3M17.5 4.5l-2 2M19.5 11.5l-2-2',
};

export function icon(name, size = 22) {
  const d = P[name] || P.more;
  return `<svg class="icon" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${d}"/></svg>`;
}
