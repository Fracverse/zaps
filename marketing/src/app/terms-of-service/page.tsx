import React from 'react';

export const metadata = {
  title: 'Terms of Service | Zaps',
  description: 'Terms of Service for Zaps',
};

export default function TermsOfServicePage() {
  return (
    <div className="max-w-4xl mx-auto px-4 py-12">
      <h1 className="text-4xl font-bold mb-8">Terms of Service</h1>
      <div className="prose prose-invert max-w-none">
        <p className="text-gray-400 mb-8"><strong>Last Updated: [Date]</strong></p>

        <h2 className="text-2xl font-semibold mt-8 mb-4">1. Acceptance of Terms</h2>
        <p className="mb-4">By accessing or using the Zaps application ("App") and any related services (collectively, the "Services"), you agree to be bound by these Terms of Service ("Terms"). If you do not agree to these Terms, do not use the Services.</p>

        <h2 className="text-2xl font-semibold mt-8 mb-4">2. Description of Services</h2>
        <p className="mb-4">Zaps provides a platform for facilitating digital asset transactions, merchant payments, and wallet management.</p>

        <h2 className="text-2xl font-semibold mt-8 mb-4">3. User Accounts & Responsibilities</h2>
        <ul className="list-disc pl-6 mb-4 space-y-2">
          <li>You must be at least 18 years old to use the Services.</li>
          <li>You are responsible for maintaining the confidentiality of your account credentials and private keys.</li>
          <li>Zaps is a non-custodial wallet provider. We do not have access to your private keys and cannot recover your funds if you lose them.</li>
        </ul>

        <h2 className="text-2xl font-semibold mt-8 mb-4">4. Merchant Terms</h2>
        <p className="mb-4">Merchants accepting payments via Zaps agree to:</p>
        <ul className="list-disc pl-6 mb-4 space-y-2">
          <li>Comply with all applicable local and international laws.</li>
          <li>Process refunds directly with customers where applicable.</li>
          <li>Pay applicable network fees or routing fees as defined in the Merchant Dashboard.</li>
        </ul>

        <h2 className="text-2xl font-semibold mt-8 mb-4">5. Cryptocurrency Risks</h2>
        <p className="mb-4">You acknowledge that the value of cryptocurrencies is highly volatile. Zaps is not responsible for any financial losses incurred due to market fluctuations, network congestion, or smart contract vulnerabilities.</p>

        <h2 className="text-2xl font-semibold mt-8 mb-4">6. Limitation of Liability</h2>
        <p className="mb-4">To the maximum extent permitted by law, Zaps shall not be liable for any indirect, incidental, special, consequential, or punitive damages, or any loss of profits or revenues.</p>

        <h2 className="text-2xl font-semibold mt-8 mb-4">7. Dispute Resolution</h2>
        <p className="mb-4">Any disputes arising out of or relating to these Terms or the Services will be resolved through binding arbitration in accordance with the rules of [Arbitration Body].</p>
      </div>
    </div>
  );
}
