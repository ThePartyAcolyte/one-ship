import type { SetupTerminalAction } from '../types/generated/SetupTerminalAction';

export const ONBOARDING_DISMISSED_KEY = 'oneship-onboarding-dismissed-v1';

export interface IntegrationInstallGuide {
  summary: string;
  installAction?: SetupTerminalAction;
  downloadUrl?: string;
  sourceUrl: string;
  sourceLabel: string;
}

export const INTEGRATION_INSTALL_GUIDES: Record<string, IntegrationInstallGuide> = {
  desigma: {
    summary: 'One-Ship Designer (desigma fork, branch "oneship") — vector/UI design editor and MCP gateway.',
    installAction: 'install_optional_desigma',
    sourceUrl: 'https://github.com/ThePartyAcolyte/desigma',
    sourceLabel: 'ThePartyAcolyte/desigma (oneship)',
  },
  pcbcraft: {
    summary: 'PCBCraft — EDA schematic & PCB layout editor and netlist/DRC MCP server.',
    installAction: 'install_optional_pcbcraft',
    sourceUrl: 'https://github.com/tangency-labs/pcbcraft',
    sourceLabel: 'tangency-labs/pcbcraft',
  },
  solidcraft: {
    summary: 'SolidCraft — parametric 3D CAD (prebuilt binary package only).',
    downloadUrl: 'https://solidcraft.io/download/',
    sourceUrl: 'https://solidcraft.io/download/',
    sourceLabel: 'solidcraft.io/download',
  },
  rea: {
    summary: 'RE Lab (REA) — binary reverse engineering, ASAR unpacking, and Ghidra gateway.',
    installAction: 'install_optional_rea',
    sourceUrl: 'https://www.npmjs.com/package/rea-agents',
    sourceLabel: 'rea-agents (npm)',
  },
  electronics: {
    summary: 'Electronics Workbench — ngspice, sigrok-cli, PlatformIO/probe-rs, and USB udev rules.',
    installAction: 'install_optional_electronics',
    sourceUrl: 'https://github.com/ThePartyAcolyte/one-ship/blob/main/packaging/udev/99-oneship-instruments.rules',
    sourceLabel: '99-oneship-instruments.rules',
  },
};

export interface RunnerInstallGuide {
  id: string;
  label: string;
  hint: string;
  installAction: SetupTerminalAction;
}

export const RUNNER_INSTALL_GUIDES: ReadonlyArray<RunnerInstallGuide> = [
  {
    id: 'claude',
    label: 'Claude Code CLI (claude)',
    hint: 'npm install -g @anthropic-ai/claude-code',
    installAction: 'install_runner_claude',
  },
  {
    id: 'opencode',
    label: 'OpenCode CLI (opencode)',
    hint: 'curl -fsSL https://opencode.ai/install | bash',
    installAction: 'install_runner_opencode',
  },
  {
    id: 'agy',
    label: 'Google Antigravity CLI (agy)',
    hint: 'curl -fsSL https://antigravity.google/cli/install.sh | bash',
    installAction: 'install_runner_agy',
  },
];

/** Returns the recommended SolidCraft package format label for the detected host package manager. */
export function getRecommendedSolidcraftPackage(packageManager: string | null | undefined): string {
  switch ((packageManager ?? '').toLowerCase()) {
    case 'dnf':
      return 'Recommended for Fedora: RPM (.rpm)';
    case 'apt':
      return 'Recommended for Debian/Ubuntu/Mint: DEB (.deb)';
    case 'pacman':
      return 'Recommended for Arch/Omarchy: AUR (solidcraft-bin) or TAR archive';
    default:
      return 'Recommended: TAR archive (.tar.gz) or native distro package';
  }
}
