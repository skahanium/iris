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
        destructive: {
          DEFAULT: "hsl(var(--destructive))",
          foreground: "hsl(var(--destructive-foreground))",
        },
        overlay: {
          scrim: "hsl(var(--overlay-scrim))",
        },
        ring: "hsl(var(--ring))",
        editor: {
          paper: "hsl(var(--editor-paper))",
          ink: "hsl(var(--editor-ink))",
          muted: "hsl(var(--editor-muted))",
          border: "hsl(var(--editor-border))",
          code: {
            bg: "hsl(var(--editor-code-bg))",
            fg: "hsl(var(--editor-code-fg))",
          },
        },
      },
      borderRadius: {
        sm: "var(--radius-sm)",
        md: "var(--radius-md)",
        lg: "var(--radius-lg)",
        xl: "var(--radius-xl)",
        "2xl": "var(--radius-lg)",
        "3xl": "var(--radius-xl)",
      },
      boxShadow: {
        overlay: "var(--shadow-overlay)",
        floating: "var(--shadow-floating)",
      },
      fontFamily: {
        sans: [
          "Inter",
          "-apple-system",
          "BlinkMacSystemFont",
          '"Segoe UI"',
          '"Microsoft YaHei"',
          '"PingFang SC"',
          "sans-serif",
        ],
        editor: [
          "Inter",
          "-apple-system",
          "BlinkMacSystemFont",
          '"Segoe UI"',
          '"Microsoft YaHei"',
          '"PingFang SC"',
          "sans-serif",
        ],
        mono: ['"JetBrains Mono"', "ui-monospace", "monospace"],
      },
      transitionDuration: {
        fast: "var(--motion-fast)",
        base: "var(--motion-base)",
        exit: "var(--motion-exit)",
      },
      transitionTimingFunction: {
        iris: "var(--motion-ease)",
        "iris-out": "var(--motion-ease-out)",
      },
      zIndex: {
        "editor-chrome": "15",
        ai: "10",
        "overlay-scrim": "40",
        overlay: "50",
      },
    },
  },
  plugins: [],
};
