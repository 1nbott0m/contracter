import http from 'k6/http';
import { check } from 'k6';
import { baseUrl, options as scenarioOptions } from './lib/config.js';
export const options = scenarioOptions();
export default function () { const response = http.get(`${baseUrl()}/health/live`); check(response, { 'live': r => r.status === 200 }); }
