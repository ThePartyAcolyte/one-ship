export const ONBOARDING_DISMISSED_KEY = 'oneship-onboarding-dismissed-v1';

export interface IntegrationInstallGuide {
  summary: string;
  installCmd: string;
  sourceUrl: string;
  sourceLabel: string;
}

export const INTEGRATION_INSTALL_GUIDES: Record<string, IntegrationInstallGuide> = {
  desigma: {
    summary: 'One-Ship Designer (desigma fork, branch "oneship") — vector/UI design editor and MCP gateway.',
    installCmd: './install.sh --features design',
    sourceUrl: 'https://github.com/ThePartyAcolyte/desigma',
    sourceLabel: 'ThePartyAcolyte/desigma (oneship)',
  },
  pcbcraft: {
    summary: 'PCBCraft — EDA schematic & PCB layout editor and netlist/DRC MCP server.',
    installCmd: './install.sh --features pcb',
    sourceUrl: 'https://github.com/tangency-labs/pcbcraft',
    sourceLabel: 'tangency-labs/pcbcraft',
  },
  solidcraft: {
    summary: 'SolidCraft — parametric 3D CAD (prebuilt binary package only, no source fork build).',
    installCmd: './install.sh --features cad',
    sourceUrl: 'https://github.com/tangency-labs/solidcraft/releases/latest',
    sourceLabel: 'tangency-labs/solidcraft Releases',
  },
  rea: {
    summary: 'RE Lab (REA) — binary reverse engineering, ASAR unpacking, and Ghidra gateway.',
    installCmd: './install.sh --features rea  # or: npm install -g rea-agents@latest',
    sourceUrl: 'https://www.npmjs.com/package/rea-agents',
    sourceLabel: 'rea-agents (npm)',
  },
  electronics: {
    summary: 'Electronics Workbench — ngspice, sigrok-cli, PlatformIO/probe-rs, and USB udev rules.',
    installCmd: './install.sh --features electronics',
    sourceUrl: 'https://github.com/ThePartyAcolyte/one-ship/blob/main/packaging/udev/99-oneship-instruments.rules',
    sourceLabel: '99-oneship-instruments.rules',
  },
};

export interface RunnerInstallGuide {
  id: string;
  label: string;
  hint: string;
}

export const RUNNER_INSTALL_GUIDES: ReadonlyArray<RunnerInstallGuide> = [
  {
    id: 'claude',
    label: 'Claude Code CLI (claude)',
    hint: 'npm install -g @anthropic-ai/claude-code',
  },
  {
    id: 'opencode',
    label: 'OpenCode CLI (opencode)',
    hint: 'curl -fsSL https://opencode.ai/install | bash',
  },
  {
    id: 'agy',
    label: 'Google Antigravity CLI (agy)',
    hint: 'Install agy CLI on PATH and authenticate your Google account.',
  },
];
