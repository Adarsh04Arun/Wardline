import { interpolate, spring, useCurrentFrame, useVideoConfig } from "remotion";
import { colors, fonts } from "./theme";

// Fade + rise in, fade out over the last 12 frames of `duration`.
export const useEnter = (delay = 0, duration?: number) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const s = spring({ frame: frame - delay, fps, config: { damping: 200 } });
  const out =
    duration === undefined
      ? 1
      : interpolate(frame, [duration - 12, duration], [1, 0], {
          extrapolateLeft: "clamp",
          extrapolateRight: "clamp",
        });
  return { opacity: s * out, transform: `translateY(${(1 - s) * 30}px)` };
};

export const Scene: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div
    style={{
      position: "absolute",
      inset: 0,
      background: `radial-gradient(ellipse at 50% 0%, #16202c 0%, ${colors.bg} 65%)`,
      color: colors.text,
      fontFamily: fonts.sans,
      display: "flex",
      flexDirection: "column",
      alignItems: "center",
      justifyContent: "center",
      padding: 120,
    }}
  >
    {children}
  </div>
);

export const Kicker: React.FC<{ children: React.ReactNode; style?: React.CSSProperties }> = ({
  children,
  style,
}) => (
  <div
    style={{
      fontFamily: fonts.mono,
      fontSize: 28,
      letterSpacing: 4,
      textTransform: "uppercase",
      color: colors.accent,
      marginBottom: 24,
      ...style,
    }}
  >
    {children}
  </div>
);

export const Window: React.FC<{
  title: string;
  children: React.ReactNode;
  style?: React.CSSProperties;
}> = ({ title, children, style }) => (
  <div
    style={{
      background: colors.panel,
      border: `2px solid ${colors.border}`,
      borderRadius: 18,
      overflow: "hidden",
      boxShadow: "0 30px 80px rgba(0,0,0,0.5)",
      ...style,
    }}
  >
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 12,
        padding: "16px 22px",
        borderBottom: `2px solid ${colors.border}`,
        fontFamily: fonts.mono,
        fontSize: 22,
        color: colors.muted,
      }}
    >
      {["#f85149", "#f5a524", "#3fb950"].map((c) => (
        <div key={c} style={{ width: 16, height: 16, borderRadius: 8, background: c }} />
      ))}
      <span style={{ marginLeft: 16 }}>{title}</span>
    </div>
    <div style={{ padding: "28px 36px" }}>{children}</div>
  </div>
);
