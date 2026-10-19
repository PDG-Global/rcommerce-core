/// <reference types="astro/client" />

declare module 'html2pdf.js';

declare namespace App {
  interface Locals {
    runtime: {
      env: {
        RCOMMERCE_API_URL?: string;
        RCOMMERCE_API_KEY?: string;
      };
    };
  }
}
