export const TARGET: string;
export const PACKAGES: string[];
export const BUILDS: { pkg: string; features: string[] }[];
export const NETWORK_CRATES: string[];
export function packageNames(treeText: string): Set<string>;
export function findProblems(pkg: string, packagesText: string, featuresText: string): string[];
export function cargoTree(pkg: string, extra: string[]): string;
export function main(tree?: (pkg: string, extra: string[]) => string): number;
