import { es } from "../../shared/i18n/es";
import { StatusBadge } from "../../shared/ui/StatusBadge";
import { usePrivilege } from "../providers/PrivilegeProvider";

/** Aviso global cuando no se puede afirmar que la app corre con la cuenta de quien la usa. */
export function PrivilegeBanner() {
  const { phase, context } = usePrivilege();
  if (phase === "loading" || context.status === "match") return null;
  return (
    <div
      role="alert"
      className="flex flex-col gap-2 rounded-lg border border-warning bg-surface-raised p-4"
    >
      <StatusBadge tone="warning">{es.privilege.title}</StatusBadge>
      <p className="text-content-muted">{es.privilege.reasons[context.reason]}</p>
      <p className="text-content-muted">{es.privilege.blockedIntro}</p>
      <ul className="list-disc pl-6 text-content-muted">
        {es.privilege.blocked.map((item) => (
          <li key={item}>{item}</li>
        ))}
      </ul>
      <p className="text-content-muted">{es.privilege.restAvailable}</p>
      <p className="text-content-muted">{es.privilege.howToFix}</p>
    </div>
  );
}
