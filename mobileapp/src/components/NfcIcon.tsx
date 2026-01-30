import React from "react";
import Svg, { Circle, Path } from "react-native-svg";
import { COLORS } from "../constants/colors";

interface NfcIconProps {
  size?: number;
  color?: string;
  fillCircle?: boolean;
}

/**
 * Contactless / NFC payment icon - three curved arcs in a circle.
 */
export function NfcIcon({
  size = 64,
  color = COLORS.primary,
  fillCircle = false,
}: NfcIconProps) {
  const strokeWidth = Math.max(2, size * 0.06);
  const r = size / 2;
  const cx = r;
  const cy = r;

  // Contactless symbol: three arcs on the right side ( ) ) )
  const r1 = r * 0.22;
  const r2 = r * 0.48;
  const r3 = r * 0.72;
  const arc1 = `M ${cx} ${cy - r1} A ${r1} ${r1} 0 0 1 ${cx} ${cy + r1}`;
  const arc2 = `M ${cx} ${cy - r2} A ${r2} ${r2} 0 0 1 ${cx} ${cy + r2}`;
  const arc3 = `M ${cx} ${cy - r3} A ${r3} ${r3} 0 0 1 ${cx} ${cy + r3}`;

  return (
    <Svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
      {fillCircle && (
        <Circle
          cx={cx}
          cy={cy}
          r={r - strokeWidth}
          fill="white"
          stroke={color}
          strokeWidth={strokeWidth}
        />
      )}
      <Path
        d={arc1}
        fill="none"
        stroke={color}
        strokeWidth={strokeWidth}
        strokeLinecap="round"
      />
      <Path
        d={arc2}
        fill="none"
        stroke={color}
        strokeWidth={strokeWidth}
        strokeLinecap="round"
      />
      <Path
        d={arc3}
        fill="none"
        stroke={color}
        strokeWidth={strokeWidth}
        strokeLinecap="round"
      />
    </Svg>
  );
}
