// Icônes au trait de la maquette (24 x 24, couleur du texte).
const PATHS = {
  home: ["M3 10.5 12 3l9 7.5V21h-6v-6H9v6H3z"],
  pulse: ["M22 12h-4l-3 9L9 3l-3 9H2"],
  disk: [
    "M22 12H2",
    "M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z",
    "M6 16h.01M10 16h.01",
  ],
  phone: ["M7 2h10a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2z", "M12 18h.01"],
  restore: ["M3 12a9 9 0 1 0 3-6.7L3 8", "M3 3v5h5"],
  report: ["M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z", "M14 2v6h6M16 13H8M16 17H8M10 9H8"],
  check: ["M20 6 9 17l-5-5"],
  alert: ["m21.73 18-8-14a2 2 0 0 0-3.46 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3z", "M12 9v4M12 17h.01"],
  x: ["M18 6 6 18M6 6l12 12"],
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 18 }: { name: IconName; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {PATHS[name].map((d) => (
        <path key={d} d={d} />
      ))}
    </svg>
  );
}
