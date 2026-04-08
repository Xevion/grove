async function init() {
  const loading = document.getElementById("loading");
  const error = document.getElementById("error");
  const errorMessage = document.getElementById("error-message");

  if (!window.isSecureContext) {
    const msg = `Insecure context: WebGPU requires HTTPS or localhost. Current origin: ${location.origin}`;
    console.error("[grove]", msg);
    loading.style.display = "none";
    error.style.display = "flex";
    errorMessage.textContent = msg;
    return;
  }

  if (!navigator.gpu) {
    const msg = "WebGPU not available (navigator.gpu is undefined)";
    console.error("[grove]", msg);
    loading.style.display = "none";
    error.style.display = "flex";
    errorMessage.textContent = msg;
    return;
  }

  try {
    const wasm = await import("./wasm/grove.js");
    await wasm.default();
    loading.style.display = "none";
    await wasm.run();
  } catch (e) {
    loading.style.display = "none";
    error.style.display = "flex";
    errorMessage.textContent = e.message || "Failed to initialize WASM module.";
    console.error("Grove WASM init failed:", e);
  }
}

init();
