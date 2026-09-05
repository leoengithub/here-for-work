import type { MouseEvent, ReactNode } from "react";
import { openExternalUrl } from "./api";

interface RoleTitleLinkProps {
  href: string;
  className?: string;
  id?: string;
  children: ReactNode;
}

export function RoleTitleLink({ href, className, id, children }: RoleTitleLinkProps) {
  return (
    <a
      id={id}
      className={className}
      href={href}
      rel="noreferrer"
      onClick={(event: MouseEvent<HTMLAnchorElement>) => {
        if (!("__TAURI_INTERNALS__" in window)) return;
        event.preventDefault();
        void openExternalUrl(href);
      }}
    >
      {children}
    </a>
  );
}
