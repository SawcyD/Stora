/**
 * Fluent-compatible line icons.
 *
 * Drawn inline as 16px SVG strokes so they match the weight of the system
 * icon set and inherit `currentColor`. No emoji anywhere in the interface.
 */

interface IconProps {
  size?: number;
  className?: string;
}

function Icon({
  size = 16,
  className,
  children,
}: IconProps & { children: React.ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      className={className}
    >
      {children}
    </svg>
  );
}

export const HomeIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M2.5 7 8 2.5 13.5 7v6a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1V7Z" />
    <path d="M6.5 14V9.5h3V14" />
  </Icon>
);

export const StorageIcon = (props: IconProps) => (
  <Icon {...props}>
    <rect x="2" y="3.5" width="12" height="4" rx="1" />
    <rect x="2" y="8.5" width="12" height="4" rx="1" />
    <path d="M4.5 5.5h.01M4.5 10.5h.01" />
  </Icon>
);

export const CleanupIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M3 4.5h10" />
    <path d="M5.5 4.5V3a.5.5 0 0 1 .5-.5h4a.5.5 0 0 1 .5.5v1.5" />
    <path d="M4.5 4.5 5 13a.5.5 0 0 0 .5.5h5A.5.5 0 0 0 11 13l.5-8.5" />
  </Icon>
);

export const AppsIcon = (props: IconProps) => (
  <Icon {...props}>
    <rect x="2.5" y="2.5" width="4.5" height="4.5" rx="1" />
    <rect x="9" y="2.5" width="4.5" height="4.5" rx="1" />
    <rect x="2.5" y="9" width="4.5" height="4.5" rx="1" />
    <rect x="9" y="9" width="4.5" height="4.5" rx="1" />
  </Icon>
);

export const FileIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M4 2.5h5L12 5.5v8a.5.5 0 0 1-.5.5h-7a.5.5 0 0 1-.5-.5v-11Z" />
    <path d="M9 2.5v3h3" />
  </Icon>
);

export const DuplicatesIcon = (props: IconProps) => (
  <Icon {...props}>
    <rect x="2.5" y="2.5" width="8" height="8" rx="1" />
    <path d="M5.5 13.5h7a1 1 0 0 0 1-1v-7" />
  </Icon>
);

export const DeveloperIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M5.5 5 2.5 8l3 3" />
    <path d="M10.5 5l3 3-3 3" />
  </Icon>
);

export const HistoryIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M2.5 8a5.5 5.5 0 1 0 1.7-4" />
    <path d="M2.5 2.5V5h2.5" />
    <path d="M8 5v3l2 1.5" />
  </Icon>
);

export const AutomationIcon = (props: IconProps) => (
  <Icon {...props}>
    <circle cx="8" cy="8" r="2" />
    <path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M12.6 3.4l-1.4 1.4M4.8 11.2l-1.4 1.4" />
  </Icon>
);

export const SettingsIcon = (props: IconProps) => (
  <Icon {...props}>
    <circle cx="8" cy="8" r="2.2" />
    <path d="M8 1.5 9 3.3a5 5 0 0 1 1.4.6l2-.5.9 1.6-1.4 1.5a5 5 0 0 1 0 1.6l1.4 1.5-.9 1.6-2-.5a5 5 0 0 1-1.4.6L8 14.5l-1-1.8a5 5 0 0 1-1.4-.6l-2 .5-.9-1.6 1.4-1.5a5 5 0 0 1 0-1.6L2.7 5.4l.9-1.6 2 .5A5 5 0 0 1 7 3.3Z" />
  </Icon>
);

export const AboutIcon = (props: IconProps) => (
  <Icon {...props}>
    <circle cx="8" cy="8" r="5.5" />
    <path d="M8 7.2v3.6" />
    <path d="M8 5.2h.01" />
  </Icon>
);

export const FolderIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M2 4.5a1 1 0 0 1 1-1h3l1.2 1.5h5.8a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1v-7.5Z" />
  </Icon>
);

export const ChevronRightIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M6 3.5 10.5 8 6 12.5" />
  </Icon>
);

export const ChevronDownIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M3.5 6 8 10.5 12.5 6" />
  </Icon>
);

export const ExternalIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M9.5 2.5h4v4" />
    <path d="M13.5 2.5 7.5 8.5" />
    <path d="M11.5 9.5v3a1 1 0 0 1-1 1h-7a1 1 0 0 1-1-1v-7a1 1 0 0 1 1-1h3" />
  </Icon>
);

export const RefreshIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M13.5 8a5.5 5.5 0 1 1-1.7-4" />
    <path d="M13.5 2.5V5h-2.5" />
  </Icon>
);
