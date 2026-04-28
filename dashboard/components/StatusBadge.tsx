"use client";
import { statusColor } from "@/lib/utils";

interface Props {
  status: string;
}

export default function StatusBadge({ status }: Props) {
  return (
    <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium capitalize ${statusColor(status)}`}>
      {status}
    </span>
  );
}
