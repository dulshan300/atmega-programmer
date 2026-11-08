import * as RDialog from "@radix-ui/react-dialog";
import clsx from "clsx";
import { X, type LucideIcon } from "lucide-react";
import { forwardRef, useId, type ButtonHTMLAttributes, type ReactNode } from "react";
import { useSyncedState } from "@/lib/hooks";

// ---------------------------------------------------------------- buttons

type Variant = "primary" | "secondary" | "ghost" | "danger";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: "sm" | "md";
  icon?: LucideIcon;
  busy?: boolean;
}

const variants: Record<Variant, string> = {
  primary: "bg-accent text-accent-fg hover:bg-accent-strong border-transparent font-medium",
  secondary: "bg-raised text-text hover:border-line-strong border-line",
  ghost: "bg-transparent text-muted hover:text-text hover:bg-raised border-transparent",
  danger: "bg-danger/90 text-white hover:bg-danger border-transparent font-medium",
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  {
    variant = "secondary",
    size = "md",
    icon: Icon,
    busy,
    className,
    children,
    disabled,
    type = "button",
    ...rest
  },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      disabled={disabled || busy}
      aria-busy={busy || undefined}
      className={clsx(
        "inline-flex items-center justify-center gap-1.5 rounded border whitespace-nowrap transition-colors",
        "disabled:opacity-45 disabled:pointer-events-none",
        size === "sm" ? "h-6 px-2 text-xs" : "h-7 px-3 text-[13px]",
        variants[variant],
        className,
      )}
      {...rest}
    >
      {Icon && <Icon size={size === "sm" ? 13 : 14} className={clsx(busy && "animate-spin")} aria-hidden />}
      {children}
    </button>
  );
});

export function IconButton({
  icon: Icon,
  label,
  active,
  className,
  ...rest
}: { icon: LucideIcon; label: string; active?: boolean } & ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      aria-pressed={active}
      className={clsx(
        "inline-flex h-7 w-7 items-center justify-center rounded text-muted transition-colors",
        "hover:bg-raised hover:text-text disabled:opacity-40 disabled:pointer-events-none",
        active && "bg-raised text-accent",
        className,
      )}
      {...rest}
    >
      <Icon size={15} aria-hidden />
    </button>
  );
}

// ---------------------------------------------------------------- form controls

const fieldBase =
  "h-7 w-full rounded border border-line bg-panel-2 px-2 text-[13px] text-text placeholder:text-faint " +
  "hover:border-line-strong focus:border-accent focus:outline-none disabled:opacity-50 aria-[invalid=true]:border-danger";

export const Input = forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(function Input(
  { className, ...rest },
  ref,
) {
  return <input ref={ref} className={clsx(fieldBase, className)} spellCheck={false} {...rest} />;
});

/**
 * Integer input that keeps local text while typing and commits valid values
 * on blur/Enter. Invalid text is shown with an error border and not committed.
 */
export function IntInput({
  value,
  onCommit,
  min,
  max,
  className,
  ...rest
}: {
  value: number;
  onCommit: (v: number) => void;
  min?: number;
  max?: number;
  className?: string;
} & Omit<React.InputHTMLAttributes<HTMLInputElement>, "value" | "onChange" | "min" | "max">) {
  const [text, setText] = useSyncedState(String(value));
  const parsed = /^-?\d+$/.test(text.trim()) ? Number(text.trim()) : NaN;
  const valid =
    Number.isSafeInteger(parsed) &&
    (min === undefined || parsed >= min) &&
    (max === undefined || parsed <= max);
  const commit = () => {
    if (valid && parsed !== value) onCommit(parsed);
    else if (!valid) setText(String(value));
  };
  return (
    <Input
      inputMode="numeric"
      value={text}
      aria-invalid={!valid}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
      className={clsx("font-mono", className)}
      title={min !== undefined || max !== undefined ? `${min ?? "−∞"} … ${max ?? "∞"}` : undefined}
      {...rest}
    />
  );
}

export function Select<T extends string>({
  value,
  onChange,
  options,
  className,
  disabled,
  ...rest
}: {
  value: T;
  onChange: (v: T) => void;
  options: { value: T; label: string; disabled?: boolean }[];
  className?: string;
  disabled?: boolean;
} & Omit<React.SelectHTMLAttributes<HTMLSelectElement>, "value" | "onChange">) {
  return (
    <select
      value={value}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value as T)}
      className={clsx(fieldBase, "pr-6", className)}
      {...rest}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value} disabled={o.disabled}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

export function Switch({
  checked,
  onChange,
  label,
  disabled,
  description,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: ReactNode;
  disabled?: boolean;
  description?: ReactNode;
}) {
  const id = useId();
  return (
    <div className={clsx("flex items-start gap-2", disabled && "opacity-50")}>
      <button
        id={id}
        type="button"
        role="switch"
        aria-checked={checked}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={clsx(
          "relative mt-0.5 h-4 w-7 shrink-0 rounded-full border transition-colors",
          checked ? "border-accent bg-accent" : "border-line-strong bg-raised",
        )}
      >
        <span
          className={clsx(
            "absolute top-[1px] h-3 w-3 rounded-full bg-white shadow transition-transform",
            checked ? "translate-x-[13px]" : "translate-x-[1px]",
          )}
        />
      </button>
      <label htmlFor={id} className="cursor-pointer">
        <div className="text-[13px]">{label}</div>
        {description && <div className="text-xs text-muted">{description}</div>}
      </label>
    </div>
  );
}

export function Field({
  label,
  hint,
  error,
  children,
  className,
}: {
  label: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <label className={clsx("flex flex-col gap-1", className)}>
      <span className="text-xs font-medium text-muted">{label}</span>
      {children}
      {error ? (
        <span className="text-xs text-danger">{error}</span>
      ) : (
        hint && <span className="text-xs text-faint">{hint}</span>
      )}
    </label>
  );
}

// ---------------------------------------------------------------- display

export function Badge({
  tone = "neutral",
  children,
  className,
}: {
  tone?: "neutral" | "accent" | "ok" | "warn" | "danger" | "info";
  children: ReactNode;
  className?: string;
}) {
  const tones = {
    neutral: "bg-raised text-muted border-line",
    accent: "bg-accent/15 text-accent border-accent/30",
    ok: "bg-ok/15 text-ok border-ok/30",
    warn: "bg-warn/15 text-warn border-warn/30",
    danger: "bg-danger/15 text-danger border-danger/30",
    info: "bg-info/15 text-info border-info/30",
  };
  return (
    <span
      className={clsx(
        "inline-flex items-center gap-1 rounded border px-1.5 py-px text-[11px] font-medium",
        tones[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}

export function Meter({
  value,
  max,
  label,
  detail,
}: {
  value: number;
  max: number;
  label: string;
  detail?: string;
}) {
  const pct = max > 0 ? Math.min(100, (value / max) * 100) : 0;
  const over = value > max;
  const tone = over ? "bg-danger" : pct > 90 ? "bg-warn" : "bg-accent";
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-baseline justify-between text-xs">
        <span className="font-medium">{label}</span>
        <span className={clsx("font-mono", over ? "text-danger" : "text-muted")}>
          {detail ?? `${value} / ${max}`} · {((value / Math.max(1, max)) * 100).toFixed(1)}%
        </span>
      </div>
      <div
        className="h-2 overflow-hidden rounded-full bg-raised"
        role="meter"
        aria-label={label}
        aria-valuenow={value}
        aria-valuemin={0}
        aria-valuemax={max}
      >
        <div className={clsx("h-full rounded-full transition-[width]", tone)} style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}

export function EmptyState({
  icon: Icon,
  title,
  children,
  action,
}: {
  icon: LucideIcon;
  title: string;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="flex h-full min-h-40 flex-col items-center justify-center gap-2 p-6 text-center">
      <Icon size={28} className="text-faint" aria-hidden />
      <div className="font-medium">{title}</div>
      {children && <div className="max-w-md text-xs text-muted">{children}</div>}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

export function SectionTitle({ children, actions }: { children: ReactNode; actions?: ReactNode }) {
  return (
    <div className="flex h-8 items-center justify-between gap-2 px-3 text-[11px] font-semibold tracking-wider text-muted uppercase">
      <span className="truncate">{children}</span>
      {actions && <div className="flex items-center gap-0.5 normal-case">{actions}</div>}
    </div>
  );
}

export function Card({
  title,
  actions,
  children,
  className,
}: {
  title?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={clsx("rounded-md border border-line bg-panel", className)}>
      {(title || actions) && (
        <header className="flex min-h-9 items-center justify-between gap-2 border-b border-line px-3 py-1.5">
          <h3 className="text-[13px] font-semibold">{title}</h3>
          {actions && <div className="flex items-center gap-1.5">{actions}</div>}
        </header>
      )}
      <div className="p-3">{children}</div>
    </section>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded border border-line bg-raised px-1 font-mono text-[10px] text-muted">
      {children}
    </kbd>
  );
}

export function Callout({
  tone,
  title,
  children,
}: {
  tone: "info" | "warn" | "danger" | "ok";
  title?: ReactNode;
  children: ReactNode;
}) {
  const t = {
    info: "border-info/40 bg-info/10",
    warn: "border-warn/40 bg-warn/10",
    danger: "border-danger/40 bg-danger/10",
    ok: "border-ok/40 bg-ok/10",
  }[tone];
  return (
    <div
      className={clsx("rounded border px-3 py-2 text-xs", t)}
      role={tone === "danger" ? "alert" : undefined}
    >
      {title && <div className="mb-0.5 font-semibold text-text">{title}</div>}
      <div className="text-muted">{children}</div>
    </div>
  );
}

// ---------------------------------------------------------------- dialog

export function Dialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  footer,
  width = 520,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
  width?: number;
}) {
  return (
    <RDialog.Root open={open} onOpenChange={onOpenChange}>
      <RDialog.Portal>
        <RDialog.Overlay className="fixed inset-0 z-40 bg-black/50" />
        <RDialog.Content
          className="fixed top-1/2 left-1/2 z-50 flex max-h-[85vh] -translate-x-1/2 -translate-y-1/2 flex-col rounded-lg border border-line bg-panel shadow-2xl"
          style={{ width: `min(${width}px, 94vw)` }}
          aria-describedby={description ? undefined : undefined}
        >
          <header className="flex items-start justify-between gap-4 border-b border-line px-4 py-3">
            <div>
              <RDialog.Title className="text-sm font-semibold">{title}</RDialog.Title>
              {description ? (
                <RDialog.Description className="mt-0.5 text-xs text-muted">{description}</RDialog.Description>
              ) : (
                <RDialog.Description className="sr-only">{title}</RDialog.Description>
              )}
            </div>
            <RDialog.Close asChild>
              <IconButton icon={X} label="Close" />
            </RDialog.Close>
          </header>
          <div className="min-h-0 flex-1 overflow-auto p-4">{children}</div>
          {footer && (
            <footer className="flex justify-end gap-2 border-t border-line px-4 py-3">{footer}</footer>
          )}
        </RDialog.Content>
      </RDialog.Portal>
    </RDialog.Root>
  );
}
