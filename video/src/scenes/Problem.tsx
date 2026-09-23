import { colors } from "../theme";
import { Kicker, Scene, useEnter } from "../ui";

const ITEMS = [
  ["Sidecar service", "another thing to deploy and page on"],
  ["Network hop", "latency on every request"],
  ["Async runtime", "pulled into code that never needed one"],
];

export const Problem: React.FC<{ duration: number }> = ({ duration }) => (
  <Scene>
    <div style={useEnter(0, duration)}>
      <Kicker style={{ textAlign: "center" }}>The usual guardrail</Kicker>
      <div style={{ fontSize: 72, fontWeight: 700, textAlign: "center", marginBottom: 70 }}>
        is a whole other system.
      </div>
    </div>
    <div style={{ display: "flex", gap: 40 }}>
      {ITEMS.map(([head, body], i) => (
        <div
          key={head}
          style={{
            ...useEnter(20 + i * 12, duration),
            width: 460,
            padding: 40,
            borderRadius: 20,
            background: colors.panel,
            border: `2px solid ${colors.border}`,
          }}
        >
          <div style={{ fontSize: 44, fontWeight: 700, color: colors.block }}>✕ {head}</div>
          <div style={{ fontSize: 30, color: colors.muted, marginTop: 16 }}>{body}</div>
        </div>
      ))}
    </div>
    <div style={{ ...useEnter(70, duration), fontSize: 48, marginTop: 80 }}>
      Wardline is <span style={{ color: colors.accent, fontWeight: 700 }}>a function call.</span>
    </div>
  </Scene>
);
