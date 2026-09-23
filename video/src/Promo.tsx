import { AbsoluteFill, Series } from "remotion";
import { Title } from "./scenes/Title";
import { Problem } from "./scenes/Problem";
import { PipelineFlow } from "./scenes/PipelineFlow";
import { CodeScene } from "./scenes/CodeScene";
import { TerminalScene } from "./scenes/TerminalScene";
import { Reliability } from "./scenes/Reliability";
import { Outro } from "./scenes/Outro";

// Scene lengths in frames at 30 fps. Each scene gets its own length as a
// prop so it can fade out over its last frames.
const SCENES = [
  [Title, 120],
  [Problem, 150],
  [PipelineFlow, 240],
  [CodeScene, 210],
  [TerminalScene, 180],
  [Reliability, 210],
  [Outro, 150],
] as const;

export const PROMO_DURATION = SCENES.reduce((n, [, d]) => n + d, 0);

export const Promo: React.FC = () => (
  <AbsoluteFill style={{ background: "#0b0f14" }}>
    <Series>
      {SCENES.map(([Component, duration], i) => (
        <Series.Sequence key={i} durationInFrames={duration}>
          <Component duration={duration} />
        </Series.Sequence>
      ))}
    </Series>
  </AbsoluteFill>
);
