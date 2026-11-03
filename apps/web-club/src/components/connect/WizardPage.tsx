import type { ReactNode } from 'react';
import ConnectNav from './ConnectNav';
import { Stepper, WizardFooter } from './WizardKit';
import { useLocale } from '../../connect/locale';

/**
 * Chrome shared by the three onboarding steps: marketing nav on top,
 * wizard header (title, subtitle, stepper), the body, and the sticky
 * footer bar with the auto-save hint and the primary action.
 */
export default function WizardPage({
  step,
  title,
  subtitle,
  children,
  backTo,
  backLabel,
  hint,
  nextLabel,
  onNext,
  nextDisabled,
  loading,
}: {
  step: number;
  title: string;
  subtitle: string;
  children: ReactNode;
  backTo?: string;
  backLabel?: string;
  hint?: string;
  nextLabel: string;
  onNext: () => void;
  nextDisabled?: boolean;
  loading?: boolean;
}) {
  const { t } = useLocale();
  const steps = [t('wizardClub'), t('wizardVenue'), t('wizardQr')];
  return (
    <div className="connect-root wizard-root flex min-h-screen flex-col bg-drop-bg">
      <ConnectNav variant="wizard" />

      <main key={step} className="wizard-main mx-auto w-full max-w-[1440px] flex-1 animate-step-in px-4 pb-8 sm:px-6 lg:px-12">
        <header className="wizard-heading">
          <div className="min-w-0">
            <p className="connect-eyebrow">0{step} / 03 · {steps[step - 1]}</p>
            <h1 className="connect-display mt-3 text-[30px] sm:text-[38px] lg:text-[42px]">
              {title}
            </h1>
            <p className="mt-3 max-w-[620px] text-[14px] leading-[1.6] text-drop-muted">{subtitle}</p>
          </div>
          <div className="wizard-stepper-wrap">
            <Stepper steps={steps} current={step} />
          </div>
        </header>

        <div className="wizard-body flex flex-col gap-5 lg:flex-row lg:items-start">{children}</div>
      </main>

      <div className="mt-auto">
        <WizardFooter
          backTo={backTo}
          backLabel={backLabel}
          hint={hint}
          nextLabel={nextLabel}
          onNext={onNext}
          nextDisabled={nextDisabled}
          loading={loading}
        />
      </div>
    </div>
  );
}
