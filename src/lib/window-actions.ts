export interface NativeFullscreenWindow {
  isFullscreen: () => Promise<boolean>;
  setFullscreen: (fullscreen: boolean) => Promise<void>;
}

export interface MaximizableWindow {
  toggleMaximize: () => Promise<void>;
}

export async function toggleNativeFullscreen(
  win: NativeFullscreenWindow,
): Promise<boolean> {
  const nextFullscreen = !(await win.isFullscreen());
  await win.setFullscreen(nextFullscreen);
  return nextFullscreen;
}

export async function toggleWindowMaximize(
  win: MaximizableWindow,
): Promise<void> {
  await win.toggleMaximize();
}
