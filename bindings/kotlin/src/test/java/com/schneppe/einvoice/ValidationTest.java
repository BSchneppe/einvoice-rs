package com.schneppe.einvoice;


import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertThrows;


class ValidationTest {

    @Test
    void validateInvoiceShouldBeValid() throws Exception {
        Path path = Paths.get(this.getClass().getClassLoader().getResource("01.11a-INVOICE_ubl.xml").getPath());
        String xml = new String(Files.readAllBytes(path));
        InvoiceStandard invoiceStandard = EinvoiceKt.validateInvoice(xml);

        assertInstanceOf(InvoiceStandard.Ubl.class, invoiceStandard);
    }

    @Test
    void validateInvoiceShouldBeInvalid() throws Exception {
        Path path = Paths.get(this.getClass().getClassLoader().getResource("no_customizationid_01.01_comprehensive_test_ubl.xml").getPath());
        String xml = new String(Files.readAllBytes(path));

        assertThrows(InvoiceException.class, () -> EinvoiceKt.validateInvoice(xml));
    }

    @Test
    void validateInvoiceShouldBeValidCii() throws Exception {
        Path path = Paths.get(this.getClass().getClassLoader().getResource("01.01_comprehensive_test_uncefact.xml").getPath());
        String xml = new String(Files.readAllBytes(path));

        InvoiceStandard invoiceStandard = EinvoiceKt.validateInvoice(xml);

        assertInstanceOf(InvoiceStandard.Cii.class, invoiceStandard);

    }
}
