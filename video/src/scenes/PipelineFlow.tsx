import { interpolate, useCurrentFrame } from "remotion";
import { colors, fonts } from "../theme";
import { Kicker, Scene, useEnter } from "../ui";

const GUARDS = ["RegexFilter", "RateLimit", "Pii", "PromptInjection"];
const GUARD_X = [500, 790, 1080, 1370];
const LANE_Y = 200;

// One input travelling left to right through the guards. With `blockAt` it
// stops at that guard and turns red; otherwise it exits to the action.
const Packet: React.FC<{ start: number; label: string; blockAt?: number }> = ({
  start,
  label,
  blockAt,
}) => {
  const frame = useCurrentFrame() - start;
  const endX = blockAt === undefined ? 1640 : GUARD_X[blockAt];
  const x = interpolate(frame, [0, 70], [180, endX], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  const arrived = frame >= 70;
  const blocked = blockAt !== undefined && arrived;
  const opacity = interpolate(frame, [0, 8, 100, 115], [0, 1, 1, 0], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  const color = blocked ? colors.block : arrived ? colors.allow : colors.blue;
  return (
    <div
      style={{
        position: "absolute",
        left: x - 200,
        top: LANE_Y - 30,
        width: 400,
        opacity,
        textAlign: "center",
      }}
    >
      <div
        style={{
          display: "inline-block",
          padding: "12px 22px",
          borderRadius: 14,
          background: color,
          color: colors.bg,
          fontFamily: fonts.mono,
          fontSize: 24,
          fontWeight: 700,
          boxShadow: `0 0 40px ${color}`,
        }}
      >
        {label}
      </div>
      {arrived && (
        <div style={{ marginTop: 150, fontSize: 32, fontFamily: fonts.mono, color }}>
          {blocked ? "Verdict::Block" : "Verdict::Allow"}
        </div>
      )}
    </div>
  );
};

export const PipelineFlow: React.FC<{ duration: number }> = ({ duration }) => {
  const fade = useEnter(0, duration);
  return (
    <Scene>
      <div style={{ ...fade, position: "absolute", top: 110, width: "100%", textAlign: "center" }}>
        <Kicker>Pipeline::evaluate()</Kicker>
        <div style={{ fontSize: 60, fontWeight: 700 }}>
          Every guard returns a <span style={{ color: colors.accent }}>blocking verdict</span>.
        </div>
      </div>
      <div style={{ ...fade, position: "absolute", left: 0, top: 440, width: 1920, height: 560 }}>
        <div
          style={{
            position: "absolute",
            left: 180,
            width: 1460,
            top: LANE_Y - 2,
            height: 4,
            background: colors.border,
          }}
        />
        <div
          style={{
            position: "absolute",
            left: 90,
            top: LANE_Y - 70,
            fontFamily: fonts.mono,
            fontSize: 24,
            color: colors.muted,
          }}
        >
          request
        </div>
        {GUARDS.map((g, i) => (
          <div
            key={g}
            style={{
              position: "absolute",
              left: GUARD_X[i] - 110,
              top: LANE_Y - 90,
              width: 220,
              textAlign: "center",
            }}
          >
            <div
              style={{
                height: 180,
                borderRadius: 20,
                border: `3px solid ${colors.accent}`,
                background: "rgba(245,165,36,0.07)",
              }}
            />
            <div style={{ fontFamily: fonts.mono, fontSize: 20, color: colors.muted, marginTop: 14 }}>
              {g}Guard
            </div>
          </div>
        ))}
        <div
          style={{
            position: "absolute",
            left: 1560,
            top: LANE_Y - 110,
            fontSize: 36,
            fontWeight: 700,
            color: colors.allow,
          }}
        >
          action ▶
        </div>
        <Packet start={20} label='"hello from wardline"' />
        <Packet start={120} label='"Ignore previous…"' blockAt={3} />
      </div>
    </Scene>
  );
};
