import { useEffect } from "react";
import { appText } from "../../text/app";
import { useAppStore } from "./store";

/** The program's name and version. */
export function AppVersion() {
  const version = useAppStore((s) => s.version);
  const error = useAppStore((s) => s.error);
  const loadVersion = useAppStore((s) => s.loadVersion);

  useEffect(() => {
    void loadVersion();
  }, [loadVersion]);

  return (
    <header>
      <h1>{appText.name}</h1>
      {error !== null ? (
        <p role="alert">{error.message}</p>
      ) : (
        <p>{version === null ? appText.loading : appText.version(version)}</p>
      )}
    </header>
  );
}
