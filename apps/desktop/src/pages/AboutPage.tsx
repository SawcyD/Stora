import { InfoRow, SectionHeader, SettingsSection } from "@sawcy/memora-ui";

import { PageHeader } from "../components/common";

const PRINCIPLES = [
  "Show exactly what consumes storage.",
  "Explain why an item may be removable.",
  "Preview every deletion before it happens.",
  "Prefer supported Windows cleanup methods.",
  "Never remove content you created without asking.",
  "Keep all analysis on this device.",
];

export default function AboutPage() {
  return (
    <>
      <PageHeader title="About" description="Understand your storage." />

      <section className="page-section">
        <SettingsSection>
          <InfoRow label="Version" value="0.1.0" />
          <InfoRow label="Edition" value="Phases 1–3" />
          <InfoRow
            label="Analysis"
            value="Local only"
            help="Stora never uploads file names, paths, or activity."
          />
        </SettingsSection>
      </section>

      <section className="page-section">
        <SectionHeader>How Stora behaves</SectionHeader>
        <SettingsSection>
          <div style={{ padding: "10px 14px" }}>
            <ul
              style={{
                margin: 0,
                paddingLeft: 18,
                fontSize: 13,
                lineHeight: 1.7,
                color: "var(--memora-text-secondary)",
              }}
            >
              {PRINCIPLES.map((principle) => (
                <li key={principle}>{principle}</li>
              ))}
            </ul>
          </div>
        </SettingsSection>
      </section>

      <section className="page-section">
        <SectionHeader>Reading the numbers</SectionHeader>
        <SettingsSection>
          <InfoRow
            label="Size on disk"
            value="Space actually occupied"
            help="Differs from logical size for compressed, sparse, and very small files."
          />
          <InfoRow
            label="Potentially recoverable"
            value="An upper bound, not a promise"
            help="Files in use at the moment of cleanup are skipped, and their space is not counted as recovered."
          />
          <InfoRow
            label="Space recovered"
            value="Removed files only"
            help="Stora never counts a file it failed to remove."
          />
          <InfoRow
            label="Last access time"
            value="Not used as proof of use"
            help="Windows may update this lazily or not at all, so Stora does not treat it as evidence."
          />
        </SettingsSection>
      </section>
    </>
  );
}
