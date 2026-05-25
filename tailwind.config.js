/** @type {import('tailwindcss').Config} */
export default {
  darkMode: ["class"],
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        background: "hsl(var(--background))",
        foreground: "hsl(var(--foreground))",
        panel: "hsl(var(--panel))",
        card: "hsl(var(--card))",
        border: "hsl(var(--border))",
        primary: {
          DEFAULT: "hsl(var(--primary))",
          foreground: "hsl(var(--primary-foreground))",
        },
        muted: {
          DEFAULT: "hsl(var(--muted))",
          foreground: "hsl(var(--muted-foreground))",
        },
        editor: {
          paper: "hsl(var(--editor-paper))",
          ink: "hsl(var(--editor-ink))",
          muted: "hsl(var(--editor-muted))",
          border: "hsl(var(--editor-border))",
        },
      },
      fontFamily: {
        editor: [
          '"Noto Serif SC"',
          '"Source Han Serif SC"',
          '"Songti SC"',
          "Georgia",
          '"Times New Roman"',
          "serif",
        ],
        mono: ['"JetBrains Mono"', "ui-monospace", "monospace"],
        sans: [
          "-apple-system",
          "BlinkMacSystemFont",
          '"Segoe UI"',
          '"Microsoft YaHei"',
          '"PingFang SC"',
          "sans-serif",
        ],
      },
    },
  },
  plugins: [],
};
