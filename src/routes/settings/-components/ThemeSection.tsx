import { Moon, Sun } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { toggleTheme, useTheme } from "@/theme";

export function ThemeSection() {
  const theme = useTheme();

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Theme</CardTitle>
        <CardDescription>
          Switch between light and dark appearance.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex items-center gap-2">
        <Button
          variant={theme === "light" ? "default" : "outline"}
          size="sm"
          onClick={() => theme === "dark" && toggleTheme()}
        >
          <Sun className="size-4" />
          Light
        </Button>
        <Button
          variant={theme === "dark" ? "default" : "outline"}
          size="sm"
          onClick={() => theme === "light" && toggleTheme()}
        >
          <Moon className="size-4" />
          Dark
        </Button>
      </CardContent>
    </Card>
  );
}
