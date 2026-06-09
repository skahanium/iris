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
        input: "hsl(var(--input))",
        primary: {
          DEFAULT: "hsl(var(--primary))",
          foreground: "hsl(var(--primary-foreground))",
        },
        secondary: {
          DEFAULT: "hsl(var(--secondary))",
          foreground: "hsl(var(--secondary-foreground))",
        },
        accent: {
          DEFAULT: "hsl(var(--accent))",
          foreground: "hsl(var(--accent-foreground))",
        },
        popover: {
          DEFAULT: "hsl(var(--popover))",
          foreground: "hsl(var(--popover-foreground))",
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
        surface: {
          chrome: "hsl(var(--surface-chrome))",
          elevated: "hsl(var(--surface-elevated))",
          inset: "hsl(var(--surface-inset))",
        },
        knowledge: {
          accent: "hsl(var(--knowledge-accent))",
          foreground: "hsl(var(--knowledge-accent-foreground))",
        },
        rail: {
          bg: "hsl(var(--iris-rail-bg))",
          active: "hsl(var(--iris-rail-active))",
          hover: "hsl(var(--iris-rail-hover))",
        },
        outline: {
          bg: "hsl(var(--outline-rail-bg))",
          active: "hsl(var(--outline-rail-active))",
        },
        task: {
          header: "hsl(var(--overlay-task-header))",
          selected: "hsl(var(--overlay-task-selected))",
        },
        command: {
          highlight: "hsl(var(--command-highlight-bg))",
          ring: "hsl(var(--command-highlight-ring))",
        },
        ai: {
          user: "hsl(var(--ai-user-bg))",
          composer: "hsl(var(--ai-composer-bg))",
          pulse: "hsl(var(--ai-stream-pulse))",
          citation: "hsl(var(--ai-citation))",
          "citation-hover": "hsl(var(--ai-citation-hover))",
        },
        "ai-workspace": {
          DEFAULT: "hsl(var(--ai-workspace-bg))",
          border: "hsl(var(--ai-workspace-border))",
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
        prose: [
          '"Noto Sans SC"',
          "Inter",
          '"PingFang SC"',
          '"Microsoft YaHei"',
          "sans-serif",
        ],
        title: ['"Noto Serif SC"', '"Noto Sans SC"', "Georgia", "serif"],
        editor: [
          '"Noto Sans SC"',
          "Inter",
          '"PingFang SC"',
          '"Microsoft YaHei"',
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
        toolbar: "20",
        "slash-command": "25",
        "overlay-scrim": "40",
        overlay: "50",
      },
    },
  },
  plugins: [],
};
