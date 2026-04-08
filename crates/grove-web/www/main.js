async function init() {
  const loading = document.getElementById("loading");
  const error = document.getElementById("error");
  const errorMessage = document.getElementById("error-message");

  try {
    const wasm = await import("./wasm/grove_web.js");
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
