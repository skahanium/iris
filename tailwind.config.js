/** @type {import('tailwindcss').Config} */
export default {
  // Iris uses dark-by-default CSS variables; `.light` is the opt-in theme class.
  // Avoid adding `dark:` utilities unless the theme strategy is changed.
  darkMode: ["class"],
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        background: "hsl(var(--background))",
        foreground: "hsl(var(--foreground))",
        panel: "hsl(var(--panel))",
        card: "hsl(var(--card))",
        border: {
          DEFAULT: "hsl(var(--border))",
          subtle: "hsl(var(--border-subtle))",
          strong: "hsl(var(--border-strong))",
        },
        input: "hsl(var(--input))",
        primary: {
          DEFAULT: "hsl(var(--primary))",
          foreground: "hsl(var(--primary-foreground))",
        },
        brand: {
          DEFAULT: "hsl(var(--brand))",
          foreground: "hsl(var(--brand-foreground))",
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
        warning: {
          DEFAULT: "hsl(var(--warning))",
          bg: "hsl(var(--warning-bg))",
          foreground: "hsl(var(--warning-fg))",
        },
        success: {
          DEFAULT: "hsl(var(--success))",
          bg: "hsl(var(--success-bg))",
          foreground: "hsl(var(--success-fg))",
        },
        "classified-accent": "hsl(var(--classified-accent))",
        status: {
          "llm-ready": "hsl(var(--status-llm-ready))",
          "llm-missing": "hsl(var(--status-llm-missing))",
          "llm-error": "hsl(var(--status-llm-error))",
          "search-api": "hsl(var(--status-search-api))",
          "search-fallback": "hsl(var(--status-search-fallback))",
          "web-search": "hsl(var(--status-web-search))",
          inactive: "hsl(var(--status-inactive))",
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
        rail: "var(--radius-rail)",
        // Radius aliases intentionally cap oversized rounded utilities to Iris tokens.
        "2xl": "var(--radius-lg)",
        "3xl": "var(--radius-xl)",
      },
      boxShadow: {
        overlay: "var(--shadow-overlay)",
        floating: "var(--shadow-floating)",
        // Map default utilities to Iris floating so leftover shadow-sm/md stay on-token.
        sm: "var(--shadow-floating)",
        md: "var(--shadow-floating)",
      },
      fontSize: {
        micro: ["var(--text-micro)", { lineHeight: "1.35" }],
        caption: ["var(--text-caption)", { lineHeight: "1.4" }],
        ui: ["var(--text-ui)", { lineHeight: "1.45" }],
        body: ["var(--text-body)", { lineHeight: "1.5" }],
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
          '"PingFang SC"',
          '"Microsoft YaHei"',
          "Inter",
          "sans-serif",
        ],
        title: [
          "Inter",
          "-apple-system",
          "BlinkMacSystemFont",
          '"Segoe UI"',
          '"PingFang SC"',
          '"Microsoft YaHei"',
          '"Noto Sans SC"',
          "sans-serif",
        ],
        editor: [
          '"Noto Sans SC"',
          '"PingFang SC"',
          '"Microsoft YaHei"',
          "Inter",
          "sans-serif",
        ],
        mono: ['"JetBrains Mono"', "ui-monospace", "monospace"],
      },
      keyframes: {
        "iris-fade-in": {
          from: { opacity: "0" },
          to: { opacity: "1" },
        },
        "iris-fade-out": {
          from: { opacity: "1" },
          to: { opacity: "0" },
        },
        // Overlay panels center with -translate-x/y-1/2. Never animate `transform`
        // (scale/slide) on those shells — it overrides translate and flashes off-center.
      },
      animation: {
        "iris-fade-in":
          "iris-fade-in var(--motion-base) var(--motion-ease-out)",
        "iris-fade-out": "iris-fade-out var(--motion-exit) var(--motion-ease)",
        // Opacity-only: safe for centered overlays and floating menus alike.
        "iris-enter": "iris-fade-in var(--motion-base) var(--motion-ease-out)",
        "iris-exit": "iris-fade-out var(--motion-exit) var(--motion-ease)",
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
