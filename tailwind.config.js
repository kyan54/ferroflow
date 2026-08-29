/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "media",
  theme: {
    extend: {
      fontFamily: {
        sans: ["IBM Plex Sans", "system-ui", "-apple-system", "Segoe UI", "sans-serif"],
        mono: ["IBM Plex Mono", "ui-monospace", "SFMono-Regular", "Menlo", "monospace"],
        display: ["Space Grotesk", "IBM Plex Sans", "system-ui", "sans-serif"],
      },
      colors: {
        border: "hsl(var(--border))",
        input: "hsl(var(--input))",
        ring: "hsl(var(--ring))",
        background: "hsl(var(--bg))",

        // Conduit tokens: three-tier text hierarchy, layered surfaces, hairline.
        surface: {
          DEFAULT: "hsl(var(--surface) / <alpha-value>)",
          2: "hsl(var(--surface-2) / <alpha-value>)",
          3: "hsl(var(--surface-3) / <alpha-value>)",
        },
        line: "hsl(var(--line) / <alpha-value>)",
        hair: "hsl(var(--hair) / <alpha-value>)",
        fg: {
          DEFAULT: "hsl(var(--fg) / <alpha-value>)",
          dim: "hsl(var(--fg-dim) / <alpha-value>)",
          faint: "hsl(var(--fg-faint) / <alpha-value>)",
        },

        // Brand + semantic colors.
        flow: {
          DEFAULT: "hsl(var(--flow) / <alpha-value>)",
          hi: "hsl(var(--flow-hi) / <alpha-value>)",
          weak: "hsl(var(--flow-weak) / <alpha-value>)",
        },
        ok: {
          DEFAULT: "hsl(var(--ok) / <alpha-value>)",
          weak: "hsl(var(--ok-weak) / <alpha-value>)",
        },
        warn: {
          DEFAULT: "hsl(var(--warn) / <alpha-value>)",
          weak: "hsl(var(--warn-weak) / <alpha-value>)",
        },
        err: {
          DEFAULT: "hsl(var(--err) / <alpha-value>)",
          weak: "hsl(var(--err-weak) / <alpha-value>)",
        },
        dn: "hsl(var(--dn) / <alpha-value>)",
        up: "hsl(var(--up) / <alpha-value>)",

        // Protocol badge hues (Servers list tags).
        "badge-blue": "hsl(var(--badge-blue) / <alpha-value>)",
        "badge-purple": "hsl(var(--badge-purple) / <alpha-value>)",
        "badge-orange": "hsl(var(--badge-orange) / <alpha-value>)",
        "badge-teal": "hsl(var(--badge-teal) / <alpha-value>)",
        "badge-indigo": "hsl(var(--badge-indigo) / <alpha-value>)",
      },
      borderRadius: {
        lg: "var(--radius)",
        md: "calc(var(--radius) - 2px)",
        sm: "calc(var(--radius) - 4px)",
      },
    },
  },
  plugins: [],
};
