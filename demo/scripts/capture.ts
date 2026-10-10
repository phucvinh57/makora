import { mkdir, copyFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { chromium, type Page } from "playwright-core";
import {
  aiReviewGroups,
  aiReviewRows,
  buildStats,
  categories,
  rules,
  sessions,
  settings,
} from "./fixtures";

const output = fileURLToPath(new URL("../public/screens/", import.meta.url));
const fonts = fileURLToPath(new URL("../public/fonts/", import.meta.url));
await mkdir(output, { recursive: true });
await mkdir(fonts, { recursive: true });
await copyFile(
  fileURLToPath(
    new URL(
      "../../node_modules/@fontsource-variable/geist/files/geist-latin-wght-normal.woff2",
      import.meta.url,
    ),
  ),
  `${fonts}/geist.woff2`,
);
await copyFile(
  fileURLToPath(
    new URL(
      "../../node_modules/@fontsource-variable/geist/LICENSE",
      import.meta.url,
    ),
  ),
  `${fonts}/LICENSE`,
);

const browser = await chromium.launch({
  executablePath: process.env.CHROME_PATH ?? "/usr/bin/google-chrome",
  headless: true,
});

try {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 1.5,
    colorScheme: "light",
    locale: "en-US",
    timezoneId: "Asia/Ho_Chi_Minh",
    reducedMotion: "reduce",
  });
  await context.addInitScript(
    (data) => {
      localStorage.setItem("makora-theme", "light");
      const target = window as typeof window & {
        __TAURI_INTERNALS__: {
          invoke: (
            command: string,
            args?: Record<string, unknown>,
          ) => Promise<unknown>;
          transformCallback: () => number;
          unregisterCallback: () => void;
        };
        __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => void };
      };
      let callbackId = 0;
      target.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        unregisterListener: () => {},
      };
      target.__TAURI_INTERNALS__ = {
        transformCallback: () => ++callbackId,
        unregisterCallback: () => {},
        invoke: async (command, args = {}) => {
          switch (command) {
            case "plugin:autostart|is_enabled":
              return false;
            case "plugin:event|listen":
              return ++callbackId;
            case "plugin:event|unlisten":
              return null;
            case "category_list":
              return data.categories;
            case "category_rule_list":
              return data.rules;
            case "settings_get":
              return data.settings;
            case "activity_stats":
              return data.stats;
            case "categorization_status":
              return { running: false, recheck: false, done: 0, total: 0 };
            case "categorization_next_auto_run":
              return null;
            case "ai_model_download_status":
              return { running: false, model: "", downloaded: 0, total: 0 };
            case "ai_model_is_loaded":
              return false;
            case "ai_model_list_installed":
              return [
                data.settings.ai.llm_model,
                data.settings.ai.embedding_model,
              ];
            case "ai_review_feedback_stats":
              return {
                corrected: 0,
                confirmed: 0,
                pinned: 1,
                rejected: 0,
                pending_review: data.aiReviewRows.length,
                pending_seconds: data.aiReviewRows.reduce(
                  (total, row) => total + row.total_seconds,
                  0,
                ),
                reviewed_seconds: 0,
              };
            case "ai_review_list": {
              const filter = args.filter as
                | { activities?: number[] }
                | undefined;
              const rows = filter?.activities
                ? data.aiReviewRows.filter((row) =>
                    filter.activities?.includes(row.activity_id),
                  )
                : data.aiReviewRows;
              return { rows, total: rows.length };
            }
            case "ai_review_list_groups": {
              const filter = args.filter as { category?: number } | undefined;
              const groups = filter?.category
                ? data.aiReviewGroups.filter(
                    (group) => group.category_id === filter.category,
                  )
                : data.aiReviewGroups;
              return { groups, total: groups.length };
            }
            case "suggestion_sources":
              return data.stats.apps.map((app) => ({
                kind: app.kind,
                key: app.label,
                name: app.label,
                total_seconds: app.total_seconds,
              }));
            case "session_list": {
              const search = String(args.search ?? "").toLowerCase();
              const rows = data.sessions.filter((row) =>
                [row.app_name, row.title, row.url ?? ""].some((value) =>
                  value.toLowerCase().includes(search),
                ),
              );
              const offset =
                Number(args.page ?? 0) * Number(args.pageSize ?? 50);
              return {
                rows: rows.slice(offset, offset + Number(args.pageSize ?? 50)),
                total: rows.length,
              };
            }
            default:
              throw new Error(`Unsupported demo command: ${command}`);
          }
        },
      };
    },
    {
      aiReviewGroups,
      aiReviewRows,
      categories,
      rules,
      sessions,
      settings,
      stats: buildStats(sessions),
    },
  );

  const errors: string[] = [];
  const page = await context.newPage();
  page.on("pageerror", (error) => errors.push(error.message));
  const baseUrl = process.env.DEMO_APP_URL ?? "http://127.0.0.1:4173";

  async function capture(page: Page, name: string) {
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(1500);
    const failure = await page
      .getByText(/Failed to load|Unsupported demo command/)
      .count();
    if (failure > 0 || errors.length > 0)
      throw new Error(`Capture ${name} failed: ${errors.join(", ")}`);
    await page.screenshot({
      path: `${output}/${name}.png`,
      animations: "disabled",
    });
    process.stdout.write(`Captured ${name}\n`);
  }

  await page.goto(`${baseUrl}/statistics`);
  await page.getByText("Total tracked time", { exact: true }).waitFor();
  await capture(page, "statistics");
  await page.getByRole("link", { name: "History", exact: true }).click();
  await page
    .getByText("A walk through Kyoto", { exact: true })
    .first()
    .waitFor();
  await capture(page, "history");
  await page.getByPlaceholder("Search app, title, or site…").fill("github");
  await page
    .getByRole("navigation", { name: "pagination" })
    .getByText(/14 sessions/)
    .waitFor();
  await capture(page, "history-sites");
  await page.getByRole("link", { name: "Categories", exact: true }).click();
  await page.getByRole("button", { name: "Edit Work", exact: true }).waitFor();
  await capture(page, "categories");
  await page.getByRole("tab", { name: "Rules", exact: true }).click();
  await page.getByText("github.com", { exact: true }).waitFor();
  await capture(page, "rules");
  await page.getByRole("tab", { name: /AI review/ }).click();
  await page.getByText("Language model", { exact: true }).waitFor();
  await capture(page, "ai-review");
} finally {
  await browser.close();
}
