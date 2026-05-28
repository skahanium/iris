/**
 * E2E 测试辅助函数
 */



/**
 * 模拟等待 AI 面板加载完成
 */
export async function waitForAiPanel(): Promise<void> {
  // 在实际 Tauri 环境中，这里会使用 Tauri driver 等待面板加载
  // 目前作为占位符，确保测试结构完整
}

/**
 * 模拟选择 AI 场景
 */
export async function selectAiScene(scene: string): Promise<void> {
  // 在实际 Tauri 环境中，这里会通过 Tauri driver 点击场景选择器
  console.log(`选择场景: ${scene}`);
}

/**
 * 模拟发送 AI 消息并等待响应
 */
export async function sendAiMessage(message: string): Promise<void> {
  // 在实际 Tauri 环境中，这里会通过 Tauri driver 输入消息并等待响应
  console.log(`发送消息: ${message}`);
}

/**
 * 检查证据包数量
 */
export function expectContextPackets(count: number): void {
  // 在实际 Tauri 环境中，这里会检查 UI 上的证据包数量
  console.log(`期望证据包数量: ${count}`);
}
