import { useCallback, useEffect, useState } from "react";

export type ToastTone = "default" | "success" | "error";

export interface ToastMessage {
  id: number;
  message: string;
  tone: ToastTone;
}

interface ToastOptions {
  tone?: ToastTone;
}

type ToastListener = (messages: ToastMessage[]) => void;

let nextToastId = 1;
let toastMessages: ToastMessage[] = [];
const toastListeners = new Set<ToastListener>();

function emitToastChange() {
  const snapshot = toastMessages.slice();
  toastListeners.forEach((listener) => listener(snapshot));
}

function pushToast(message: string, options: ToastOptions = {}) {
  const toast: ToastMessage = {
    id: nextToastId,
    message,
    tone: options.tone ?? "default",
  };
  nextToastId += 1;
  toastMessages = [...toastMessages.slice(-2), toast];
  emitToastChange();

  window.setTimeout(() => {
    toastMessages = toastMessages.filter((item) => item.id !== toast.id);
    emitToastChange();
  }, 2200);
}

export function useToast() {
  return useCallback((message: string, options?: ToastOptions) => {
    pushToast(message, options);
  }, []);
}

export function useToastMessages() {
  const [messages, setMessages] = useState<ToastMessage[]>(toastMessages);

  useEffect(() => {
    toastListeners.add(setMessages);
    return () => {
      toastListeners.delete(setMessages);
    };
  }, []);

  return messages;
}
