import type {
  AiReviewGroup,
  AiReviewRow,
  ActivityStats,
  Category,
  CategoryRule,
  SessionRow,
  Settings,
} from "../../src/types";

export const categories: Category[] = [
  {
    id: 1,
    name: "Work",
    color: "#7561e8",
    parent_id: null,
    description: "Building things that matter.",
  },
  {
    id: 2,
    name: "Development",
    color: "#7561e8",
    parent_id: 1,
    description: "Writing code and reviewing changes.",
  },
  {
    id: 3,
    name: "Planning",
    color: "#a18af0",
    parent_id: 1,
    description: "Projects, notes, and team coordination.",
  },
  {
    id: 4,
    name: "Learning",
    color: "#20a58d",
    parent_id: null,
    description: "Reading, research, and new ideas.",
  },
  {
    id: 5,
    name: "Personal",
    color: "#e9a04e",
    parent_id: null,
    description: "A little room for everything else.",
  },
];

const activities: {
  app: string;
  key: string;
  title: string;
  url: string | null;
  category: number;
  minutes: number;
  ai?: boolean;
  confidence?: number;
}[] = [
  {
    app: "Visual Studio Code",
    key: "code",
    title: "Dashboard.tsx — weekend-project",
    url: null,
    category: 2,
    minutes: 82,
  },
  {
    app: "Chrome",
    key: "github.com",
    title: "Review pull request · weekend-project",
    url: "https://github.com/example/weekend-project/pull/42",
    category: 2,
    minutes: 34,
  },
  {
    app: "Chrome",
    key: "linear.app",
    title: "Plan the next release",
    url: "https://linear.app/example/team/all",
    category: 3,
    minutes: 24,
  },
  {
    app: "Chrome",
    key: "developer.mozilla.org",
    title: "JavaScript guide | MDN",
    url: "https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide",
    category: 4,
    minutes: 28,
  },
  {
    app: "Visual Studio Code",
    key: "code",
    title: "styles.css — weekend-project",
    url: null,
    category: 2,
    minutes: 61,
  },
  {
    app: "Obsidian",
    key: "obsidian",
    title: "Ideas for the next project",
    url: null,
    category: 3,
    minutes: 19,
    ai: true,
    confidence: 0.87,
  },
  {
    app: "Chrome",
    key: "github.com",
    title: "Browse issues · weekend-project",
    url: "https://github.com/example/weekend-project/issues",
    category: 2,
    minutes: 17,
  },
  {
    app: "Chrome",
    key: "youtube.com",
    title: "A walk through Kyoto",
    url: "https://www.youtube.com/watch?v=sample",
    category: 5,
    minutes: 26,
  },
  {
    app: "Terminal",
    key: "gnome-terminal",
    title: "weekend-project — shell",
    url: null,
    category: 2,
    minutes: 12,
  },
  {
    app: "Chrome",
    key: "wikipedia.org",
    title: "Urban design — Wikipedia",
    url: "https://en.wikipedia.org/wiki/Urban_design",
    category: 4,
    minutes: 18,
    ai: true,
    confidence: 0.81,
  },
];

export const sessions: SessionRow[] = [];
for (let dayIndex = 0; dayIndex < 7; dayIndex++) {
  let minuteOfDay = 9 * 60;
  for (const [activityIndex, item] of activities.entries()) {
    const minutes = Math.round(
      item.minutes * [0.48, 0.6, 1, 0.92, 1.08, 0.85, 1.02][dayIndex],
    );
    const day = String(dayIndex + 2).padStart(2, "0");
    const start =
      new Date(`2026-10-${day}T00:00:00+07:00`).getTime() +
      minuteOfDay * 60_000;
    const category = categories.find((value) => value.id === item.category);
    sessions.push({
      id: sessions.length + 1,
      activity_id: activityIndex + 1,
      app_name: item.app,
      source_kind: item.url ? "domain" : "exe",
      source_key: item.key,
      title: item.title,
      url: item.url,
      start_time: new Date(start).toISOString(),
      end_time: new Date(start + minutes * 60_000).toISOString(),
      category_id: item.category,
      category_name: category?.name ?? null,
      category_color: category?.color ?? null,
      category_is_pinned: activityIndex === 7,
      category_source: item.ai ? "ai" : activityIndex === 7 ? "manual" : "rule",
    });
    minuteOfDay += minutes + 3;
  }
}
sessions.reverse();

export function buildStats(rows: SessionRow[]): ActivityStats {
  const stats: ActivityStats = {
    total_seconds: 0,
    session_count: rows.length,
    categories: [],
    apps: [],
    daily: [],
  };
  for (const row of rows) {
    const seconds =
      (new Date(row.end_time ?? row.start_time).getTime() -
        new Date(row.start_time).getTime()) /
      1000;
    stats.total_seconds += seconds;
    let category = stats.categories.find(
      (item) => item.category_id === row.category_id,
    );
    if (!category) {
      category = {
        category_id: row.category_id,
        name: row.category_name,
        color: row.category_color,
        total_seconds: 0,
        session_count: 0,
      };
      stats.categories.push(category);
    }
    category.total_seconds += seconds;
    category.session_count++;
    const label = row.source_kind === "domain" ? row.source_key : row.app_name;
    let app = stats.apps.find((item) => item.label === label);
    if (!app) {
      app = {
        label,
        kind: row.source_kind,
        category_color: row.category_color,
        total_seconds: 0,
        session_count: 0,
      };
      stats.apps.push(app);
    }
    app.total_seconds += seconds;
    app.session_count++;
    const day = new Date(new Date(row.start_time).getTime() + 7 * 3_600_000)
      .toISOString()
      .slice(0, 10);
    let daily = stats.daily.find((item) => item.day === day);
    if (!daily) {
      daily = { day, total_seconds: 0 };
      stats.daily.push(daily);
    }
    daily.total_seconds += seconds;
  }
  stats.apps.sort((a, b) => b.total_seconds - a.total_seconds);
  stats.daily.sort((a, b) => a.day.localeCompare(b.day));
  return stats;
}

export const aiReviewRows: AiReviewRow[] = activities.flatMap(
  (item, activityIndex) => {
    if (!item.ai) return [];
    const activityId = activityIndex + 1;
    const activitySessions = sessions.filter(
      (row) => row.activity_id === activityId,
    );
    const first = activitySessions[0];
    const category = categories.find((value) => value.id === item.category);
    if (!first || !category) return [];
    return [
      {
        activity_id: activityId,
        app_name: item.app,
        url: item.url,
        title: item.title,
        category_id: category.id,
        category_name: category.name,
        category_color: category.color,
        total_seconds: activitySessions.reduce(
          (total, row) =>
            total +
            (new Date(row.end_time ?? row.start_time).getTime() -
              new Date(row.start_time).getTime()) /
              1000,
          0,
        ),
        session_count: activitySessions.length,
      },
    ];
  },
);

export const aiReviewGroups: AiReviewGroup[] = aiReviewRows.map((row) => {
  const item = activities[row.activity_id - 1];
  return {
    key_kind: item.url ? "domain" : "exe",
    key: item.key,
    app_name: item.app,
    category_id: row.category_id,
    category_name: row.category_name,
    category_color: row.category_color,
    activity_count: 1,
    total_seconds: row.total_seconds,
    min_confidence: item.confidence ?? null,
    activity_ids: [row.activity_id],
  };
});

export const rules: CategoryRule[] = (
  [
    { id: 1, kind: "app", field: null, value: "code", category_id: 2 },
    { id: 2, kind: "site", field: null, value: "github.com", category_id: 2 },
    { id: 3, kind: "site", field: null, value: "linear.app", category_id: 3 },
    {
      id: 4,
      kind: "site",
      field: null,
      value: "developer.mozilla.org",
      category_id: 4,
    },
  ] as const
).map((rule) => {
  const matched = sessions.filter((row) => row.source_key === rule.value);
  return {
    ...rule,
    activity_count: new Set(matched.map((row) => row.activity_id)).size,
    total_seconds: buildStats(matched).total_seconds,
  };
});

export const settings: Settings = {
  tracking: {
    min_session_seconds: 5,
    excluded: [{ kind: "site", value: "bank.example" }],
  },
  ai: {
    llm_model: "qwen3:4b",
    embedding_model: "embeddinggemma",
    auto_categorize_hours: 0,
  },
};
