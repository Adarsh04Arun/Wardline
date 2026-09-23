import { colors, fonts } from "../theme";
import { Kicker, Scene, useEnter } from "../ui";

// Each claim maps to docs/RELIABILITY.md or the criterion baseline in
// docs/ARCHITECTURE.md; keep them in sync.
const PILLARS = [
  ["Panic isolation", "catch_unwind around every guard → GuardError::Panicked"],
  ["Fail-closed default", "a guard that errors blocks unless it opts out"],
  ["Strict deadlines", "strict guards get DeadlineViolated, not a leak"],
  ["Always a trace", "bounded audit trace, even on allow and on panic"],
];

export const Reliability: React.FC<{ duration: number }> = ({ duration }) => (
  <Scene>
    <div style={{ ...useEnter(0, duration), textAlign: "center", marginBottom: 60 }}>
      <Kicker>Hardened for reliability-sensitive code</Kicker>
      <div style={{ fontSize: 60, fontWeight: 700 }}>
        Every guarantee is backed by a <span style={{ color: colors.accent }}>named test</span>.
      </div>
    </div>
    <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 32, width: 1500 }}>
      {PILLARS.map(([head, body], i) => (
        <div
          key={head}
          style={{
            ...useEnter(20 + i * 10, duration),
            padding: "34px 40px",
            borderRadius: 20,
            background: colors.panel,
            borderLeft: `6px solid ${colors.allow}`,
          }}
        >
          <div style={{ fontSize: 40, fontWeight: 700 }}>✓ {head}</div>
          <div style={{ fontSize: 26, fontFamily: fonts.mono, color: colors.muted, marginTop: 12 }}>
            {body}
          </div>
        </div>
      ))}
    </div>
    <div style={{ ...useEnter(80, duration), marginTop: 60, fontSize: 40, color: colors.muted }}>
      ~<span style={{ color: colors.accent, fontWeight: 700 }}>221 ns</span> for an isolated no-op
      guard through <span style={{ fontFamily: fonts.mono }}>evaluate</span>
    </div>
  </Scene>
);
