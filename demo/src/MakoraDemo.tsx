import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import {
  AbsoluteFill,
  cancelRender,
  continueRender,
  delayRender,
  Img,
  interpolate,
  Sequence,
  spring,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import { scenes } from "./timing";

const ink = "#242036";
const muted = "#716b82";
const purple = "#7052d9";
const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;

function Hourglass({ size = 32 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      role="img"
      aria-label="Makora hourglass"
    >
      <path d="M5 3h14M5 21h14M6 3v3a6 6 0 0 0 3 5l3 1 3-1a6 6 0 0 0 3-5V3M6 21v-3a6 6 0 0 1 3-5l3-1 3 1a6 6 0 0 1 3 5v3" />
    </svg>
  );
}

function Brand({ large = false }: { large?: boolean }) {
  return (
    <div
      style={{ display: "flex", alignItems: "center", gap: large ? 20 : 12 }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          width: large ? 72 : 44,
          height: large ? 72 : 44,
          borderRadius: large ? 19 : 12,
          background: purple,
          color: "white",
        }}
      >
        <Hourglass size={large ? 42 : 27} />
      </div>
      <span
        style={{
          fontSize: large ? 54 : 30,
          fontWeight: 650,
          letterSpacing: -1,
        }}
      >
        Makora
      </span>
    </div>
  );
}

function Reveal({
  children,
  delay = 0,
  style,
}: {
  children: ReactNode;
  delay?: number;
  style?: CSSProperties;
}) {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const progress = spring({
    frame: frame - delay,
    fps,
    config: { damping: 200 },
    durationInFrames: 28,
  });
  return (
    <div
      style={{
        opacity: progress,
        transform: `translateY(${(1 - progress) * 24}px)`,
        ...style,
      }}
    >
      {children}
    </div>
  );
}

function Scene({
  children,
  duration,
}: {
  children: ReactNode;
  duration: number;
}) {
  const frame = useCurrentFrame();
  return (
    <AbsoluteFill
      style={{
        opacity: interpolate(
          frame,
          [0, 10, duration - 10, duration - 1],
          [0, 1, 1, 0],
          clamp,
        ),
      }}
    >
      {children}
    </AbsoluteFill>
  );
}

function Heading({
  step,
  title,
  description,
}: {
  step: string;
  title: string;
  description: string;
}) {
  return (
    <div
      style={{
        position: "absolute",
        left: 96,
        right: 96,
        top: 44,
        display: "flex",
        alignItems: "flex-start",
        gap: 48,
      }}
    >
      <div style={{ paddingTop: 14 }}>
        <Brand />
      </div>
      <div style={{ borderLeft: "1px solid #dcd6e9", paddingLeft: 44 }}>
        <div
          style={{
            color: purple,
            fontSize: 17,
            fontWeight: 600,
            letterSpacing: 2,
            textTransform: "uppercase",
            marginBottom: 8,
          }}
        >
          {step}
        </div>
        <div
          style={{
            fontSize: 43,
            fontWeight: 650,
            letterSpacing: -1.7,
            lineHeight: 1.15,
          }}
        >
          {title}
        </div>
        <div style={{ color: muted, fontSize: 24, marginTop: 10 }}>
          {description}
        </div>
      </div>
    </div>
  );
}

function AppWindow({
  screen,
  style,
}: {
  screen: string;
  style?: CSSProperties;
}) {
  return (
    <div
      style={{
        borderRadius: 16,
        overflow: "hidden",
        border: "1px solid #d7d1e4",
        background: "white",
        boxShadow: "0 24px 70px #4132681c, 0 2px 8px #4132680c",
        ...style,
      }}
    >
      <div
        style={{
          height: 34,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          background: "#f9f8fb",
          borderBottom: "1px solid #e6e2ec",
          position: "relative",
        }}
      >
        <div
          style={{ position: "absolute", left: 16, display: "flex", gap: 7 }}
        >
          {["#d7d1e0", "#d7d1e0", "#d7d1e0"].map((color, index) => (
            <span
              key={index}
              style={{
                width: 9,
                height: 9,
                borderRadius: "50%",
                background: color,
              }}
            />
          ))}
        </div>
        <span style={{ color: muted, fontSize: 13, fontWeight: 500 }}>
          Makora
        </span>
      </div>
      <Img
        src={staticFile(`screens/${screen}.png`)}
        style={{ width: "100%", display: "block" }}
      />
    </div>
  );
}

function ScreenStage({ screen }: { screen: string }) {
  return (
    <Reveal
      delay={5}
      style={{ position: "absolute", width: 1280, left: 320, top: 185 }}
    >
      <AppWindow screen={screen} />
    </Reveal>
  );
}

function Footer({ children }: { children?: ReactNode }) {
  return (
    <div
      style={{
        position: "absolute",
        bottom: 26,
        left: 96,
        right: 96,
        display: "flex",
        justifyContent: "space-between",
        fontSize: 17,
        color: muted,
      }}
    >
      <span>
        {children ?? "Automatic tracking. A clearer picture of your day."}
      </span>
      <span>Illustrative activity data</span>
    </div>
  );
}

function Intro() {
  const frame = useCurrentFrame();
  return (
    <>
      <Reveal style={{ position: "absolute", left: 118, top: 122 }}>
        <Brand large />
      </Reveal>
      <Reveal delay={12} style={{ position: "absolute", left: 118, top: 292 }}>
        <div
          style={{
            color: purple,
            fontSize: 20,
            fontWeight: 600,
            letterSpacing: 2,
            textTransform: "uppercase",
          }}
        >
          Your time, made visible
        </div>
        <h1
          style={{
            fontSize: 112,
            lineHeight: 1.04,
            fontWeight: 650,
            letterSpacing: -6,
            margin: "28px 0",
          }}
        >
          Where did
          <br />
          your day go?
        </h1>
        <p style={{ color: muted, fontSize: 30, lineHeight: 1.5, margin: 0 }}>
          Meet the desktop time tracker
          <br />
          that follows your focus.
        </p>
      </Reveal>
      <Reveal
        delay={24}
        style={{
          position: "absolute",
          left: 838,
          top: 245,
          width: 1260,
          transform: `translateY(${interpolate(frame, [0, 150], [16, -7], clamp)}px) rotate(-3deg)`,
        }}
      >
        <AppWindow screen="statistics" />
      </Reveal>
      <Reveal
        delay={36}
        style={{
          position: "absolute",
          left: 118,
          bottom: 116,
          display: "flex",
          gap: 14,
        }}
      >
        {["Automatic", "Local", "No account"].map((label) => (
          <span
            key={label}
            style={{
              padding: "12px 21px",
              border: "1px solid #dbd3ec",
              borderRadius: 30,
              fontSize: 22,
              background: "#ffffff99",
            }}
          >
            {label}
          </span>
        ))}
      </Reveal>
    </>
  );
}

function Tracking() {
  const frame = useCurrentFrame();
  const active = frame < 58 ? 0 : frame < 112 ? 1 : 2;
  const apps = [
    {
      name: "Visual Studio Code",
      label: "Write a little code",
      symbol: "</>",
      color: "#7052d9",
      time: "09:00 — 09:42",
      duration: "42m",
    },
    {
      name: "github.com",
      label: "Review a change",
      symbol: "↗",
      color: "#269881",
      time: "09:42 — 09:56",
      duration: "14m",
    },
    {
      name: "Obsidian",
      label: "Capture an idea",
      symbol: "◇",
      color: "#a377ca",
      time: "09:56 — now",
      duration: "Tracking",
    },
  ];
  return (
    <>
      <Heading
        step="01 / Go about your day"
        title="No timers. Just follow your focus."
        description="Switch apps, and Makora starts a new activity block."
      />
      <div
        style={{
          position: "absolute",
          left: 160,
          top: 284,
          display: "flex",
          gap: 28,
        }}
      >
        {apps.map((app, index) => (
          <Reveal key={app.name} delay={index * 8}>
            <div
              style={{
                width: 480,
                height: 244,
                padding: 32,
                borderRadius: 24,
                background: active === index ? "white" : "#ffffff77",
                border: `2px solid ${active === index ? app.color : "#e4dfed"}`,
                boxShadow: active === index ? "0 18px 46px #47376a12" : "none",
                transform: `translateY(${active === index ? -9 : 0}px)`,
              }}
            >
              <div style={{ color: app.color, fontSize: 42, fontWeight: 600 }}>
                {app.symbol}
              </div>
              <div style={{ fontSize: 32, fontWeight: 600, marginTop: 21 }}>
                {app.name}
              </div>
              <div style={{ color: muted, fontSize: 25, marginTop: 10 }}>
                {app.label}
              </div>
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 9,
                  color: app.color,
                  fontSize: 18,
                  marginTop: 20,
                  opacity: active === index ? 1 : 0,
                }}
              >
                <span
                  style={{
                    width: 8,
                    height: 8,
                    borderRadius: "50%",
                    background: app.color,
                  }}
                />{" "}
                In focus
              </div>
            </div>
          </Reveal>
        ))}
      </div>
      <Reveal
        delay={22}
        style={{ position: "absolute", left: 350, top: 657, width: 1220 }}
      >
        <div
          style={{
            color: muted,
            fontSize: 18,
            letterSpacing: 2,
            textTransform: "uppercase",
            marginBottom: 20,
          }}
        >
          Your day becomes a history
        </div>
        {apps.slice(0, active + 1).map((app, index) => (
          <div
            key={app.name}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 22,
              padding: "21px 28px",
              marginBottom: 12,
              borderRadius: 14,
              border: "1px solid #e0dbea",
              background: "#ffffffbb",
              fontSize: 24,
            }}
          >
            <span
              style={{
                width: 10,
                height: 10,
                borderRadius: "50%",
                background: app.color,
              }}
            />
            <span style={{ flex: 1, fontWeight: 550 }}>{app.name}</span>
            <span style={{ color: muted, width: 240 }}>
              {index === active
                ? `${app.time.split(" — ")[0]} — now`
                : app.time}
            </span>
            <span
              style={{
                color: index === active ? app.color : ink,
                width: 125,
                textAlign: "right",
              }}
            >
              {index === active ? "Tracking" : app.duration}
            </span>
          </div>
        ))}
      </Reveal>
      <Footer>Website detail uses the optional browser extension.</Footer>
    </>
  );
}

function History() {
  const frame = useCurrentFrame();
  return (
    <>
      <Heading
        step="02 / Look back"
        title="Every app. Every website. One history."
        description="Search by app, title, or site to find the moments that matter."
      />
      <ScreenStage screen={frame < 92 ? "history" : "history-sites"} />
      {frame > 65 && frame < 135 && (
        <div
          style={{
            position: "absolute",
            left: 956,
            top: 246,
            width: 231,
            height: 34,
            borderRadius: 9,
            border: "3px solid #7052d9",
            boxShadow: "0 0 0 5px #7052d922",
            opacity: interpolate(
              frame,
              [65, 78, 120, 135],
              [0, 1, 1, 0],
              clamp,
            ),
          }}
        />
      )}
      <Footer>
        Separate browser time by site with the optional extension.
      </Footer>
    </>
  );
}

function Categories() {
  const frame = useCurrentFrame();
  return (
    <>
      <Heading
        step="03 / Give your time meaning"
        title="Your categories. Your rules."
        description="Group your activities, then let rules organize matching apps and sites."
      />
      <ScreenStage screen={frame < 92 ? "categories" : "rules"} />
      <Footer>
        Manual choices take priority. Optional AI suggestions run locally.
      </Footer>
    </>
  );
}

function Statistics() {
  return (
    <>
      <Heading
        step="04 / See the bigger picture"
        title="Turn a busy week into a clear picture."
        description="Explore your categories, most-used apps, websites, and daily trends."
      />
      <ScreenStage screen="statistics" />
      <Footer>Today, the past seven days, or the past thirty days.</Footer>
    </>
  );
}

function Privacy() {
  return (
    <>
      <Heading
        step="05 / Stay in control"
        title="Your activity stays on your computer."
        description="No account. Local history. Choose what gets recorded."
      />
      <Reveal style={{ position: "absolute", left: 118, top: 302, width: 445 }}>
        <div
          style={{
            width: 78,
            height: 78,
            background: "#def3e9",
            color: "#23816a",
            borderRadius: 24,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 40,
          }}
        >
          ✓
        </div>
        <h2
          style={{
            fontSize: 56,
            lineHeight: 1.12,
            letterSpacing: -2,
            fontWeight: 650,
            margin: "30px 0",
          }}
        >
          A little more
          <br />
          peace of mind.
        </h2>
        {[
          "Exclude apps, sites, or pages",
          "Skip very short sessions",
          "Pick a light or dark theme",
        ].map((label, index) => (
          <Reveal
            key={label}
            delay={18 + index * 10}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 12,
              color: muted,
              fontSize: 24,
              marginTop: 26,
            }}
          >
            <span style={{ color: "#23816a" }}>✓</span>
            {label}
          </Reveal>
        ))}
      </Reveal>
      <Reveal
        delay={12}
        style={{ position: "absolute", left: 624, top: 233, width: 1170 }}
      >
        <AppWindow screen="settings" />
      </Reveal>
      <Footer>
        Optional AI models need an initial download; categorization runs
        locally.
      </Footer>
    </>
  );
}

function Outro() {
  return (
    <>
      <Reveal
        style={{ display: "flex", justifyContent: "center", marginTop: 169 }}
      >
        <Brand large />
      </Reveal>
      <Reveal delay={12} style={{ textAlign: "center", marginTop: 64 }}>
        <div
          style={{
            fontSize: 94,
            lineHeight: 1.09,
            letterSpacing: -4.5,
            fontWeight: 650,
          }}
        >
          See where your time goes.
        </div>
        <div
          style={{
            fontSize: 94,
            lineHeight: 1.09,
            letterSpacing: -4.5,
            fontWeight: 650,
            color: purple,
            marginTop: 10,
          }}
        >
          Keep it yours.
        </div>
      </Reveal>
      <Reveal
        delay={28}
        style={{
          display: "flex",
          justifyContent: "center",
          gap: 42,
          color: muted,
          fontSize: 26,
          marginTop: 48,
        }}
      >
        {["Windows", "macOS", "Linux"].map((label) => (
          <span key={label}>{label}</span>
        ))}
      </Reveal>
      <Reveal delay={38} style={{ textAlign: "center", marginTop: 74 }}>
        <div style={{ fontSize: 20, color: muted, marginBottom: 18 }}>
          Explore Makora
        </div>
        <div
          style={{
            display: "inline-block",
            background: purple,
            color: "white",
            padding: "22px 34px",
            borderRadius: 16,
            fontSize: 31,
            fontWeight: 500,
          }}
        >
          github.com/phucvinh57/makora
        </div>
      </Reveal>
      <div
        style={{
          position: "absolute",
          bottom: 58,
          width: "100%",
          textAlign: "center",
          color: muted,
          fontSize: 20,
        }}
      >
        Automatic tracking · Local history · No account
      </div>
    </>
  );
}

const components = {
  intro: Intro,
  tracking: Tracking,
  history: History,
  categories: Categories,
  statistics: Statistics,
  privacy: Privacy,
  outro: Outro,
};

export function MakoraDemo() {
  const [fontHandle] = useState(() =>
    delayRender("Loading the bundled Geist font"),
  );
  useEffect(() => {
    const font = new FontFace(
      "Demo Geist",
      `url(${staticFile("fonts/geist.woff2")})`,
      { weight: "100 900" },
    );
    font
      .load()
      .then((loaded) => {
        document.fonts.add(loaded);
        continueRender(fontHandle);
      })
      .catch(cancelRender);
  }, [fontHandle]);

  return (
    <AbsoluteFill
      style={{
        background: "#f7f5fc",
        color: ink,
        fontFamily: '"Demo Geist", sans-serif',
        overflow: "hidden",
      }}
    >
      <AbsoluteFill
        style={{
          background:
            "radial-gradient(ellipse at 82% 28%, #e9e1fb 0%, transparent 57%), radial-gradient(ellipse at 6% 94%, #e9f1ee 0%, transparent 44%)",
        }}
      />
      {scenes.map((scene) => {
        const Component = components[scene.name];
        return (
          <Sequence
            key={scene.name}
            from={scene.start}
            durationInFrames={scene.duration}
          >
            <Scene duration={scene.duration}>
              <Component />
            </Scene>
          </Sequence>
        );
      })}
    </AbsoluteFill>
  );
}
