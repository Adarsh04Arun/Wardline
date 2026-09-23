import { interpolate, useCurrentFrame } from "remotion";
import { colors, fonts } from "../theme";
import { Kicker, Scene, useEnter } from "../ui";

export const Title: React.FC<{ duration: number }> = ({ duration }) => {
  const frame = useCurrentFrame();
  const lineWidth = interpolate(frame, [10, 45], [0, 520], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  return (
    <Scene>
      <div style={{ ...useEnter(0, duration), textAlign: "center" }}>
        <Kicker>Rust · crates.io 0.1.0</Kicker>
        <div style={{ fontSize: 180, fontWeight: 800, letterSpacing: -4 }}>
          Ward<span style={{ color: colors.accent }}>line</span>
        </div>
        <div
          style={{
            height: 6,
            width: lineWidth,
            background: colors.accent,
            margin: "20px auto 40px",
            borderRadius: 3,
          }}
        />
      </div>
      <div
        style={{
          ...useEnter(25, duration),
          fontSize: 48,
          color: colors.muted,
          textAlign: "center",
          maxWidth: 1400,
        }}
      >
        Synchronous, embeddable guardrails,{" "}
        <span style={{ color: colors.text }}>inline in your request path.</span>
      </div>
      <div
        style={{
          ...useEnter(45, duration),
          fontFamily: fonts.mono,
          marginTop: 50,
          fontSize: 30,
          color: colors.muted,
        }}
      >
        no service · no network hop · no async runtime
      </div>
    </Scene>
  );
};
