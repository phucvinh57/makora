import { Toaster as SonnerToaster } from "sonner";

function Toaster(props: React.ComponentProps<typeof SonnerToaster>) {
  return (
    <SonnerToaster
      theme="system"
      className="toaster group"
      style={
        {
          "--normal-bg": "var(--popover)",
          "--normal-text": "var(--popover-foreground)",
          "--normal-border": "var(--border)",
        } as React.CSSProperties
      }
      toastOptions={{
        // Sonner lays the icon, title/description, and action button out as
        // a single non-wrapping row by default. An action label long enough
        // to matter here — "Apply to all N app.exe activities" — then fights
        // the description column for that row's width instead of wrapping,
        // so the button visually overlaps the text rather than sitting
        // below it. Forcing the button onto its own full-width row avoids
        // that regardless of how long the label ends up being.
        classNames: {
          toast: "flex-wrap! items-start!",
          content: "flex-1! min-w-0!",
          actionButton: "basis-full! mt-2!",
        },
      }}
      {...props}
    />
  );
}

export { Toaster };
