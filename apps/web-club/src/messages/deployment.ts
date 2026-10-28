/**
 * Deployment status catalog — the fixed bottom strip.
 *
 * `components/DeploymentStatus.tsx` renders one of two visible states of the
 * single bottom strip:
 *
 *   test         open signup / mocked payments / demo data on a stand — orange;
 *   maintenance  the platform is being worked on and interruptions are possible — red.
 *
 * `normal` renders nothing, so it has no copy. The strip states the facts it
 * knows (`open signup`, `mock payments`) instead of a generic "not production":
 * a stand with live payments would otherwise claim more than it can prove.
 * It is the only deployment indicator in the product.
 */
import type { MessageCatalog } from '../i18n';

export const deploymentCatalog = {
  en: {
    'deployment.testLabel': 'Test mode',
    'deployment.signupOpen': 'open signup',
    'deployment.signupClosed': 'closed signup',
    'deployment.paymentsMock': 'mock payments',
    'deployment.paymentsLive': 'live payments',
    'deployment.maintenanceLabel': 'Maintenance',
    'deployment.maintenanceDetail': 'faults and interruptions are possible',
  },
  es: {
    'deployment.testLabel': 'Modo de prueba',
    'deployment.signupOpen': 'registro abierto',
    'deployment.signupClosed': 'registro cerrado',
    'deployment.paymentsMock': 'pagos simulados',
    'deployment.paymentsLive': 'pagos reales',
    'deployment.maintenanceLabel': 'Mantenimiento',
    'deployment.maintenanceDetail': 'pueden producirse fallos e interrupciones del servicio',
  },
  fr: {
    'deployment.testLabel': 'Mode test',
    'deployment.signupOpen': 'inscription ouverte',
    'deployment.signupClosed': 'inscription fermée',
    'deployment.paymentsMock': 'paiements simulés',
    'deployment.paymentsLive': 'paiements réels',
    'deployment.maintenanceLabel': 'Travaux techniques',
    'deployment.maintenanceDetail': 'des incidents et des interruptions de service sont possibles',
  },
  de: {
    'deployment.testLabel': 'Testmodus',
    'deployment.signupOpen': 'offene Registrierung',
    'deployment.signupClosed': 'geschlossene Registrierung',
    'deployment.paymentsMock': 'simulierte Zahlungen',
    'deployment.paymentsLive': 'echte Zahlungen',
    'deployment.maintenanceLabel': 'Wartung',
    'deployment.maintenanceDetail': 'Störungen und Unterbrechungen des Dienstes sind möglich',
  },
  pt: {
    'deployment.testLabel': 'Modo de teste',
    'deployment.signupOpen': 'registo aberto',
    'deployment.signupClosed': 'registo fechado',
    'deployment.paymentsMock': 'pagamentos simulados',
    'deployment.paymentsLive': 'pagamentos reais',
    'deployment.maintenanceLabel': 'Manutenção',
    'deployment.maintenanceDetail': 'podem ocorrer falhas e interrupções do serviço',
  },
  ru: {
    'deployment.testLabel': 'Тестовый режим работы',
    'deployment.signupOpen': 'открытая регистрация',
    'deployment.signupClosed': 'закрытая регистрация',
    'deployment.paymentsMock': 'платежи мок',
    'deployment.paymentsLive': 'живые платежи',
    'deployment.maintenanceLabel': 'Технические работы',
    'deployment.maintenanceDetail': 'возможны неполадки и перебои в работе сервиса',
  },
} satisfies MessageCatalog;