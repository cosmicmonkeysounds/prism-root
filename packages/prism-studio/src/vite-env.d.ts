/// <reference types="vite/client" />

declare module "*.png" {
  const url: string;
  export default url;
}
declare module "*.svg" {
  const url: string;
  export default url;
}
declare module "*.css";
declare module "*.md?raw" {
  const content: string;
  export default content;
}
