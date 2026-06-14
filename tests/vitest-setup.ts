/** 消除 jsdom 下 React act() 警告 */
(
  globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

/** Node 部分版本下读取 globalThis.localStorage 会触发实验性 getter 警告。 */
const localStorageStore = new Map<string, string>();
Object.defineProperty(globalThis, "localStorage", {
  configurable: true,
  value: {
    get length() {
      return localStorageStore.size;
    },
    clear() {
      localStorageStore.clear();
    },
    getItem(key: string) {
      return localStorageStore.get(key) ?? null;
    },
    setItem(key: string, value: string) {
      localStorageStore.set(key, String(value));
    },
    removeItem(key: string) {
      localStorageStore.delete(key);
    },
    key(index: number) {
      return [...localStorageStore.keys()][index] ?? null;
    },
  } satisfies Storage,
});
