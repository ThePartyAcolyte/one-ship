/**
 * Composable for accelerating press-and-hold button interactions.
 * Triggers an initial action immediately on pointerdown, waits for an initial delay,
 * then continuously repeats the action at an accelerating rate until released.
 */

export interface PressAndHoldOptions {
  /** Initial delay before repeating begins in milliseconds (default: 350ms) */
  initialDelayMs?: number;
  /** Starting repeat interval in milliseconds (default: 150ms) */
  initialIntervalMs?: number;
  /** Fastest repeat interval clamp in milliseconds (default: 25ms) */
  minIntervalMs?: number;
  /** Multiplier applied to interval on each tick (< 1.0 accelerates, default: 0.85) */
  accelerationRate?: number;
}

export interface HoldTriggerBindings {
  pointerdown: (e: PointerEvent) => void;
  pointerup: () => void;
  pointerleave: () => void;
  pointercancel: () => void;
  keydown: (e: KeyboardEvent) => void;
  keyup: (e: KeyboardEvent) => void;
}

export function usePressAndHold() {
  function createHoldTrigger(
    action: () => void,
    options: PressAndHoldOptions = {}
  ): HoldTriggerBindings {
    const initialDelay = options.initialDelayMs ?? 350;
    const initialInterval = options.initialIntervalMs ?? 150;
    const minInterval = options.minIntervalMs ?? 25;
    const acceleration = options.accelerationRate ?? 0.85;

    let delayTimer: number | null = null;
    let repeatTimer: number | null = null;
    let currentInterval = initialInterval;

    function stop() {
      if (delayTimer !== null) {
        window.clearTimeout(delayTimer);
        delayTimer = null;
      }
      if (repeatTimer !== null) {
        window.clearTimeout(repeatTimer);
        repeatTimer = null;
      }
    }

    function scheduleNext() {
      repeatTimer = window.setTimeout(() => {
        action();
        currentInterval = Math.max(minInterval, Math.round(currentInterval * acceleration));
        scheduleNext();
      }, currentInterval);
    }

    function start(e?: Event) {
      if (e) {
        e.preventDefault();
      }
      stop();

      // Trigger first step immediately
      action();

      currentInterval = initialInterval;
      delayTimer = window.setTimeout(() => {
        scheduleNext();
      }, initialDelay);
    }

    return {
      pointerdown: (e: PointerEvent) => {
        // Only react to main primary button (left mouse click or touch)
        if (e.button === 0) {
          start(e);
        }
      },
      pointerup: stop,
      pointerleave: stop,
      pointercancel: stop,
      keydown: (e: KeyboardEvent) => {
        if ((e.key === 'Enter' || e.key === ' ') && !e.repeat) {
          start(e);
        }
      },
      keyup: (e: KeyboardEvent) => {
        if (e.key === 'Enter' || e.key === ' ') {
          stop();
        }
      },
    };
  }

  return {
    createHoldTrigger,
  };
}
