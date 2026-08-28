const label = document.querySelector("#status-label");
const indicator = document.querySelector("#status-indicator");
const details = document.querySelector("#status-details");
const refresh = document.querySelector("#refresh-status");

async function loadStatus() {
  label.textContent = "Checking";
  indicator.dataset.state = "checking";
  refresh.disabled = true;

  try {
    const response = await fetch("/health", {
      headers: { Accept: "application/json" },
    });
    const payload = await response.json();

    if (!response.ok) {
      throw new Error(`Health check returned HTTP ${response.status}`);
    }

    label.textContent = "Healthy";
    indicator.dataset.state = "healthy";
    details.textContent = JSON.stringify(payload, null, 2);
  } catch (error) {
    label.textContent = "Unavailable";
    indicator.dataset.state = "error";
    details.textContent = error instanceof Error ? error.message : String(error);
  } finally {
    refresh.disabled = false;
    document.documentElement.dataset.exampleReady = "true";
  }
}

refresh.addEventListener("click", loadStatus);
loadStatus();
