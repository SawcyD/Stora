import { InfoBar } from "@sawcy/memora-ui";

import { PageHeader } from "../components/common";

interface ComingSoonPageProps {
  title: string;
  description: string;
}

/**
 * Placeholder for feature areas that arrive in a later phase.
 *
 * These pages state plainly what is not built yet rather than showing an empty
 * shell or, worse, invented figures.
 */
export default function ComingSoonPage({ title, description }: ComingSoonPageProps) {
  return (
    <>
      <PageHeader title={title} />
      <InfoBar tone="info" title="Not available in this release" message={description} />
    </>
  );
}
