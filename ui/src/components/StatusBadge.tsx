import { STATUS_HELP, STATUS_LABEL } from "../capabilities";
import type { Status } from "../types";

export function StatusBadge({ status }: { status: Status }) {
  return (
    <span className={`badge badge-${status}`} title={STATUS_HELP[status]}>
      {STATUS_LABEL[status]}
    </span>
  );
}
