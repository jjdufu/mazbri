export function App() {
  return (
    <main className="min-h-screen flex items-center justify-center bg-[#f4f6f9] text-[#1f2937] dark:bg-[#0b0e14] dark:text-[#e5e7eb]">
      <div className="text-center space-y-1">
        <h1 className="text-lg font-semibold tracking-tight">Mazbri</h1>
        <p className="text-xs opacity-60">v{__APP_VERSION__}</p>
      </div>
    </main>
  );
}

export default App;
