"use client";

import React, { useState, useEffect } from "react";
import LiveChart from "./LiveChart";
import { subscribeSnapshot } from "../lib/engine";
import type { ChartSnapshot } from "../lib/engine";

export default function PlaygroundChart() {
  const [snapshot, setSnapshot] = useState<ChartSnapshot>({ ticks: [] });

  useEffect(() => {
    // subscribeSnapshot returns an unsubscribe function
    return subscribeSnapshot(setSnapshot);
  }, []);

  return (
    <LiveChart
      data={snapshot.ticks}
      sma={snapshot.sma}
      bollinger={snapshot.bollinger}
      crosses={snapshot.crosses}
      height={400}
    />
  );
}
