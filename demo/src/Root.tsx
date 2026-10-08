import { Composition } from "remotion";
import { MakoraDemo } from "./MakoraDemo";
import { DURATION, FPS, HEIGHT, WIDTH } from "./timing";

export function RemotionRoot() {
  return (
    <Composition
      id="MakoraDemo"
      component={MakoraDemo}
      width={WIDTH}
      height={HEIGHT}
      fps={FPS}
      durationInFrames={DURATION}
    />
  );
}
