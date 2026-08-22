import { Toaster as SonnerToaster } from "sonner";
import { useTheme } from "@/theme";

function Toaster(props: React.ComponentProps<typeof SonnerToaster>) {
  const theme = useTheme();

  return (
    <SonnerToaster
      theme={theme}
      className="toaster group"
      style={
        {
          "--normal-bg": "var(--popover)",
          "--normal-text": "var(--popover-foreground)",
          "--normal-border": "var(--border)",
        } as React.CSSProperties
      }
      toastOptions={{
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
