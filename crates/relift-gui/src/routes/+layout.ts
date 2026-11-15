// The GUI is a client-side application: it talks to the Tauri backend over IPC,
// so server-side rendering is disabled and every route is prerendered.
export const ssr = false;
export const prerender = true;
