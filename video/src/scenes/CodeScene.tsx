import { useCurrentFrame } from "remotion";
import { colors, fonts } from "../theme";
import { Kicker, Scene, useEnter, Window } from "../ui";

// Lifted from the README quickstart.
const CODE = `use wardline_core::{Context, Pipeline};
use wardline_guards::PromptInjectionGuard;

let pipeline = Pipeline::new()
    .with(PromptInjectionGuard::new()?);

let verdict = pipeline.evaluate(&input, &Context::new());
if verdict.is_block() {
    return Err(Forbidden);
}`;

const KEYWORDS = /\b(use|let|if|return|fn)\b/g;

const highlight = (line: string) => {
  if (line.trim().startsWith("//")) return <span style={{ color: colors.muted }}>{line}</span>;
  const parts = line.split(KEYWORDS);
  return parts.map((p, i) =>
    ["use", "let", "if", "return", "fn"].includes(p) ? (
      <span key={i} style={{ color: "#ff7b72" }}>
        {p}
      </span>
    ) : (
      <span key={i}>
        {p.split(/(Pipeline|PromptInjectionGuard|Context)/).map((q, j) =>
          /^(Pipeline|PromptInjectionGuard|Context)$/.test(q) ? (
            <span key={j} style={{ color: colors.accent }}>
              {q}
            </span>
          ) : (
            q
          ),
        )}
      </span>
    ),
  );
};

export const CodeScene: React.FC<{ duration: number }> = ({ duration }) => {
  const frame = useCurrentFrame();
  const chars = Math.max(0, Math.floor((frame - 15) * 2.4));
  const typed = CODE.slice(0, chars);
  const cursorOn = Math.floor(frame / 15) % 2 === 0;
  return (
    <Scene>
      <div style={{ ...useEnter(0, duration), textAlign: "center", marginBottom: 50 }}>
        <Kicker>One trait. Plain Rust.</Kicker>
        <div style={{ fontSize: 60, fontWeight: 700 }}>Guards are code, not YAML.</div>
      </div>
      <Window title="src/main.rs" style={{ ...useEnter(10, duration), width: 1300 }}>
        <pre
          style={{
            margin: 0,
            fontFamily: fonts.mono,
            fontSize: 34,
            lineHeight: 1.5,
            minHeight: 510,
          }}
        >
          {typed.split("\n").map((line, i, all) => (
            <div key={i} style={{ minHeight: "1.5em" }}>
              {highlight(line)}
              {i === all.length - 1 && (
                <span style={{ opacity: cursorOn ? 1 : 0, color: colors.accent }}>▍</span>
              )}
            </div>
          ))}
        </pre>
      </Window>
    </Scene>
  );
};
