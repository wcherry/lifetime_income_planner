import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Button } from "./ui";

type ConfirmRequest = {
  kind: "confirm";
  message: string;
  confirmLabel: string;
  cancelLabel: string;
  resolve: (result: boolean) => void;
};

type PromptRequest = {
  kind: "prompt";
  message: string;
  defaultValue: string;
  confirmLabel: string;
  cancelLabel: string;
  resolve: (result: string | null) => void;
};

type DialogRequest = ConfirmRequest | PromptRequest;

interface ConfirmOptions {
  confirmLabel?: string;
  cancelLabel?: string;
}

interface ConfirmContextValue {
  confirm: (message: string, options?: ConfirmOptions) => Promise<boolean>;
  prompt: (
    message: string,
    defaultValue?: string,
    options?: ConfirmOptions,
  ) => Promise<string | null>;
}

const ConfirmContext = createContext<ConfirmContextValue | null>(null);

/** Replaces window.confirm/window.prompt with a themed dialog. Mount once near the app root. */
export function ConfirmDialogProvider({ children }: { children: ReactNode }) {
  const [request, setRequest] = useState<DialogRequest | null>(null);

  const confirm = useCallback((message: string, options?: ConfirmOptions) => {
    return new Promise<boolean>((resolve) => {
      setRequest({
        kind: "confirm",
        message,
        confirmLabel: options?.confirmLabel ?? "OK",
        cancelLabel: options?.cancelLabel ?? "Cancel",
        resolve,
      });
    });
  }, []);

  const prompt = useCallback((message: string, defaultValue = "", options?: ConfirmOptions) => {
    return new Promise<string | null>((resolve) => {
      setRequest({
        kind: "prompt",
        message,
        defaultValue,
        confirmLabel: options?.confirmLabel ?? "OK",
        cancelLabel: options?.cancelLabel ?? "Cancel",
        resolve,
      });
    });
  }, []);

  function close(result: boolean | string | null) {
    if (!request) return;
    if (request.kind === "confirm") request.resolve(result as boolean);
    else request.resolve(result as string | null);
    setRequest(null);
  }

  return (
    <ConfirmContext.Provider value={{ confirm, prompt }}>
      {children}
      {request && <DialogHost request={request} onClose={close} />}
    </ConfirmContext.Provider>
  );
}

function DialogHost({
  request,
  onClose,
}: {
  request: DialogRequest;
  onClose: (result: boolean | string | null) => void;
}) {
  const [value, setValue] = useState(request.kind === "prompt" ? request.defaultValue : "");
  const inputRef = useRef<HTMLInputElement>(null);
  const cancel = request.kind === "prompt" ? null : false;

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") onClose(cancel);
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [onClose, cancel]);

  function submit() {
    onClose(request.kind === "prompt" ? value : true);
  }

  return (
    <div className="modal-overlay" onClick={() => onClose(cancel)}>
      <div
        className="modal confirm-dialog"
        role={request.kind === "prompt" ? "dialog" : "alertdialog"}
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
      >
        <p className="confirm-dialog-message">{request.message}</p>
        {request.kind === "prompt" && (
          <input
            ref={inputRef}
            className="input confirm-dialog-input"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") submit();
            }}
          />
        )}
        <div className="confirm-dialog-actions">
          <Button variant="ghost" type="button" onClick={() => onClose(cancel)}>
            {request.cancelLabel}
          </Button>
          <Button
            variant="primary"
            type="button"
            autoFocus={request.kind !== "prompt"}
            onClick={submit}
          >
            {request.confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}

export function useConfirmDialog() {
  const ctx = useContext(ConfirmContext);
  if (!ctx) throw new Error("useConfirmDialog must be used within a ConfirmDialogProvider");
  return ctx;
}
