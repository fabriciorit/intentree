// The control panel: scenario buttons, the latency and loss sliders, the speed buttons and
// "start over". It reports what the user did through callbacks and mirrors the network settings.

export function createControls({ scenarios, onScenario, onLatency, onLoss, onSpeed, onReset }) {
  const latency = document.getElementById('latency');
  const loss = document.getElementById('loss');
  const latencyOut = document.getElementById('latency-out');
  const lossOut = document.getElementById('loss-out');
  const clockOut = document.getElementById('clock');
  const speedButtons = [...document.querySelectorAll('#speed button')];

  const holder = document.getElementById('scenario-buttons');
  for (const scenario of scenarios) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = scenario.label;
    button.addEventListener('click', () => onScenario(scenario));
    holder.append(button);
  }

  latency.addEventListener('input', () => onLatency(Number(latency.value)));
  loss.addEventListener('input', () => onLoss(Number(loss.value) / 100));
  for (const button of speedButtons) button.addEventListener('click', () => onSpeed(Number(button.dataset.speed)));
  document.getElementById('reset').addEventListener('click', onReset);

  return {
    // keeps the sliders honest when a scenario changes the network on its own
    sync(settings, nowMs) {
      if (document.activeElement !== latency) latency.value = settings.latencyMs;
      if (document.activeElement !== loss) loss.value = Math.round(settings.lossRate * 100);
      latencyOut.textContent = `${Math.round(settings.latencyMs)} ms`;
      lossOut.textContent = `${Math.round(settings.lossRate * 100)} %`;
      clockOut.textContent = `${(nowMs / 1000).toFixed(1)} s`;
    },
    showSpeed(speed) {
      for (const button of speedButtons) button.classList.toggle('is-on', Number(button.dataset.speed) === speed);
    },
  };
}
