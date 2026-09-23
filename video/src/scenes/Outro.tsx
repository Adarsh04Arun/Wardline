import { colors, fonts } from "../theme";
import { Scene, useEnter, Window } from "../ui";

export const Outro: React.FC<{ duration: number }> = ({ duration }) => (
  <Scene>
    <div style={{ ...useEnter(0, duration), fontSize: 140, fontWeight: 800, letterSpacing: -3 }}>
      Ward<span style={{ color: colors.accent }}>line</span>
    </div>
    <Window title="Cargo.toml" style={{ ...useEnter(15, duration), width: 900, marginTop: 40 }}>
      <pre style={{ margin: 0, fontFamily: fonts.mono, fontSize: 34, lineHeight: 1.6 }}>
        <span style={{ color: colors.muted }}>[dependencies]</span>
        {"\n"}wardline-core = <span style={{ color: colors.allow }}>"0.1.0"</span>
        {"\n"}wardline-guards = <span style={{ color: colors.allow }}>"0.1.0"</span>
      </pre>
    </Window>
    <div
      style={{
        ...useEnter(35, duration),
        marginTop: 60,
        fontFamily: fonts.mono,
        fontSize: 36,
        color: colors.muted,
      }}
    >
      github.com/Adarsh04Arun/Wardline
    </div>
    <div style={{ ...useEnter(45, duration), marginTop: 20, fontSize: 28, color: colors.muted }}>
      MIT OR Apache-2.0
    </div>
  </Scene>
);
